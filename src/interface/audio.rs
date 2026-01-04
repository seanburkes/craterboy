use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

use crate::domain::Emulator;

const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_VOLUME: f32 = 0.3;
const TARGET_BUFFER_MS: u32 = 30;
const MAX_BUFFER_MS: u32 = 60;
const MIN_BUFFER_FRAMES: usize = 256;

pub struct AudioOutput {
    stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    running: Arc<AtomicBool>,
    sample_rate: u32,
    target_buffer_frames: usize,
    max_buffer_frames: usize,
    samples: Arc<Mutex<VecDeque<[i16; 2]>>>,
}

impl AudioOutput {
    pub fn new() -> Self {
        let target_buffer_frames = buffer_frames_for_ms(DEFAULT_SAMPLE_RATE, TARGET_BUFFER_MS);
        let max_buffer_frames = buffer_frames_for_ms(DEFAULT_SAMPLE_RATE, MAX_BUFFER_MS);
        Self {
            stream: None,
            stream_handle: None,
            sink: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            sample_rate: DEFAULT_SAMPLE_RATE,
            target_buffer_frames: target_buffer_frames.max(MIN_BUFFER_FRAMES),
            max_buffer_frames: max_buffer_frames.max(MIN_BUFFER_FRAMES),
            samples: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn start(&mut self, emulator: &mut Emulator) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        let device_rate = rodio::cpal::default_host()
            .default_output_device()
            .and_then(|device| device.default_output_config().ok())
            .map(|config| config.sample_rate().0)
            .unwrap_or(DEFAULT_SAMPLE_RATE);
        let sample_rate = if device_rate == 0 {
            DEFAULT_SAMPLE_RATE
        } else {
            device_rate
        };

        emulator.apu_set_sample_rate_hz(sample_rate as f64);

        let (stream, stream_handle) = OutputStream::try_default().ok().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.set_volume(DEFAULT_VOLUME);
        sink.append(RingSource::new(self.samples.clone(), sample_rate));
        sink.play();

        self.sample_rate = sample_rate;
        self.target_buffer_frames =
            buffer_frames_for_ms(sample_rate, TARGET_BUFFER_MS).max(MIN_BUFFER_FRAMES);
        self.max_buffer_frames = buffer_frames_for_ms(sample_rate, MAX_BUFFER_MS)
            .max(self.target_buffer_frames * 2)
            .max(MIN_BUFFER_FRAMES);

        self.samples.lock().unwrap().clear();
        self.stream = Some(stream);
        self.stream_handle = Some(stream_handle);
        *self.sink.lock().unwrap() = Some(sink);
        self.running.store(true, Ordering::SeqCst);
    }

    pub fn enqueue_emulator_samples(&self, emulator: &mut Emulator) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        let mut drained: Vec<[i16; 2]> = Vec::new();
        while emulator.apu_has_sample() {
            let (left, right) = emulator.apu_take_sample_stereo();
            let scaled_left = (left * 256).clamp(-32768, 32767) as i16;
            let scaled_right = (right * 256).clamp(-32768, 32767) as i16;
            drained.push([scaled_left, scaled_right]);
        }

        if drained.is_empty() {
            return;
        }

        if drained.len() > self.max_buffer_frames {
            drained.drain(0..drained.len() - self.max_buffer_frames);
        }

        let mut queue = self.samples.lock().unwrap();
        while queue.len() + drained.len() > self.max_buffer_frames {
            queue.pop_front();
        }
        queue.extend(drained);
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.samples.lock().unwrap().clear();
        *self.sink.lock().unwrap() = None;
        self.stream = None;
        self.stream_handle = None;
    }

    pub fn is_playing(&self) -> bool {
        self.sink
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| !s.empty())
            .unwrap_or(false)
    }

    pub fn set_volume(&self, volume: f32) {
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.set_volume(volume);
        }
    }
}

struct RingSource {
    samples: Arc<Mutex<VecDeque<[i16; 2]>>>,
    sample_rate: u32,
    last_frame: [i16; 2],
    pending_frame: Option<[i16; 2]>,
    pending_index: u8,
}

impl RingSource {
    fn new(samples: Arc<Mutex<VecDeque<[i16; 2]>>>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
            last_frame: [0, 0],
            pending_frame: None,
            pending_index: 0,
        }
    }
}

impl Iterator for RingSource {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(frame) = self.pending_frame {
            let sample = if self.pending_index == 0 {
                self.pending_index = 1;
                frame[0]
            } else {
                self.pending_index = 0;
                self.pending_frame = None;
                frame[1]
            };
            return Some(sample);
        }

        let frame = self.samples.lock().unwrap().pop_front();
        match frame {
            Some(frame) => {
                self.last_frame = frame;
                self.pending_frame = Some(frame);
                self.pending_index = 1;
                Some(frame[0])
            }
            None => {
                let frame = self.last_frame;
                self.pending_frame = Some(frame);
                self.pending_index = 1;
                Some(frame[0])
            }
        }
    }
}

impl Source for RingSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Default for AudioOutput {
    fn default() -> Self {
        Self::new()
    }
}

fn buffer_frames_for_ms(sample_rate: u32, ms: u32) -> usize {
    if sample_rate == 0 || ms == 0 {
        return 0;
    }
    ((sample_rate as u64 * ms as u64) / 1_000) as usize
}
