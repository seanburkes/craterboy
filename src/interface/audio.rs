use crate::domain::Emulator;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const CHUNK_SIZE: usize = 512;
const DEFAULT_VOLUME: f32 = 0.3;
const MAX_QUEUED_CHUNKS: usize = 4;
const MAX_BUFFERED_SAMPLES: usize = AUDIO_SAMPLE_RATE as usize / 2;

pub struct AudioOutput {
    stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    running: Arc<AtomicBool>,
    sample_rate: u32,
    samples: Arc<Mutex<VecDeque<i16>>>,
}

impl AudioOutput {
    pub fn new() -> Self {
        Self {
            stream: None,
            stream_handle: None,
            sink: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            sample_rate: AUDIO_SAMPLE_RATE,
            samples: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn start(&mut self, sample_rate_hz: f64) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        let (stream, stream_handle) = OutputStream::try_default().ok().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.set_volume(DEFAULT_VOLUME);

        let sample_rate = sample_rate_hz.round() as u32;
        self.sample_rate = if sample_rate == 0 {
            AUDIO_SAMPLE_RATE
        } else {
            sample_rate
        };

        self.stream = Some(stream);
        self.stream_handle = Some(stream_handle);
        *self.sink.lock().unwrap() = Some(sink);
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let sink = self.sink.clone();
        let samples = self.samples.clone();
        let sample_rate = self.sample_rate;

        thread::spawn(move || {
            let mut output_buffer: Vec<i16> = Vec::with_capacity(CHUNK_SIZE * 2);

            while running.load(Ordering::SeqCst) {
                output_buffer.clear();
                {
                    let mut queue = samples.lock().unwrap();
                    let count = queue.len().min(CHUNK_SIZE);
                    for _ in 0..count {
                        if let Some(sample) = queue.pop_front() {
                            output_buffer.push(sample);
                            output_buffer.push(sample);
                        }
                    }
                }

                if output_buffer.is_empty() {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }

                if let Some(sink_guard) = sink.lock().unwrap().as_ref() {
                    while sink_guard.len() > MAX_QUEUED_CHUNKS {
                        thread::sleep(Duration::from_millis(1));
                        if !running.load(Ordering::SeqCst) {
                            return;
                        }
                    }
                    sink_guard.append(rodio::buffer::SamplesBuffer::new(
                        2,
                        sample_rate,
                        output_buffer.clone(),
                    ));
                }
            }
        });
    }

    pub fn enqueue_emulator_samples(&self, emulator: &mut Emulator) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        let mut drained: Vec<i16> = Vec::new();
        while emulator.apu_has_sample() {
            let sample = emulator.apu_take_sample() as i32;
            let scaled = (sample * 256).clamp(-32768, 32767) as i16;
            drained.push(scaled);
        }

        if drained.is_empty() {
            return;
        }

        if drained.len() > MAX_BUFFERED_SAMPLES {
            drained.drain(0..drained.len() - MAX_BUFFERED_SAMPLES);
        }

        let mut queue = self.samples.lock().unwrap();
        while queue.len() + drained.len() > MAX_BUFFERED_SAMPLES {
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

impl Default for AudioOutput {
    fn default() -> Self {
        Self::new()
    }
}
