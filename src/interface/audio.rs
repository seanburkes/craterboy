use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

use crate::domain::Emulator;

const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_VOLUME: f32 = 0.3;
const TARGET_BUFFER_MS: u32 = 60;
const MAX_BUFFER_MS: u32 = 150;
const MIN_BUFFER_FRAMES: usize = 256;
const AUDIO_READ_BATCH_FRAMES: usize = 256;

pub struct AudioOutput {
    stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    running: Arc<AtomicBool>,
    sample_rate: u32,
    target_buffer_frames: usize,
    max_buffer_frames: usize,
    samples: Arc<Mutex<VecDeque<[i16; 2]>>>,
    staged_frames: Arc<AtomicUsize>,
    underrun_frames: Arc<AtomicU64>,
    handled_underrun_frames: u64,
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
            staged_frames: Arc::new(AtomicUsize::new(0)),
            underrun_frames: Arc::new(AtomicU64::new(0)),
            handled_underrun_frames: 0,
        }
    }

    pub fn start(&mut self, emulator: &mut Emulator) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        let output_device = rodio::cpal::default_host()
            .default_output_device()
            .expect("no default audio output device");
        let output_config = output_device
            .default_output_config()
            .expect("default audio output configuration");
        let device_rate = output_config.sample_rate().0;
        let sample_rate = if device_rate == 0 {
            DEFAULT_SAMPLE_RATE
        } else {
            device_rate
        };

        emulator.apu_set_sample_rate_hz(sample_rate as f64);

        self.sample_rate = sample_rate;
        self.target_buffer_frames =
            buffer_frames_for_ms(sample_rate, TARGET_BUFFER_MS).max(MIN_BUFFER_FRAMES);
        self.max_buffer_frames = buffer_frames_for_ms(sample_rate, MAX_BUFFER_MS)
            .max(self.target_buffer_frames * 2)
            .max(MIN_BUFFER_FRAMES);
        self.samples.lock().unwrap().clear();
        self.staged_frames.store(0, Ordering::Relaxed);
        self.underrun_frames.store(0, Ordering::Relaxed);
        self.handled_underrun_frames = 0;

        let (stream, stream_handle) =
            OutputStream::try_from_device_config(&output_device, output_config)
                .expect("open default audio output stream");
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.set_volume(DEFAULT_VOLUME);
        sink.append(RingSource::new(
            self.samples.clone(),
            self.staged_frames.clone(),
            self.underrun_frames.clone(),
            sample_rate,
        ));
        sink.pause();
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
            let (left, right) = emulator.apu_take_sample_stereo_i16();
            drained.push([left, right]);
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
        let ready =
            queue.len() + self.staged_frames.load(Ordering::Relaxed) >= self.target_buffer_frames;
        drop(queue);

        if ready && let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.play();
        }
    }

    pub fn needs_refill(&self) -> bool {
        self.buffered_frames() < self.target_buffer_frames
    }

    pub fn buffered_ms(&self) -> f64 {
        self.buffered_frames() as f64 * 1_000.0 / f64::from(self.sample_rate)
    }

    pub fn underrun_frames(&self) -> u64 {
        self.underrun_frames.load(Ordering::Relaxed)
    }

    pub fn pause_for_underrun_recovery(&mut self) -> bool {
        let underruns = self.underrun_frames();
        if underruns == self.handled_underrun_frames {
            return false;
        }

        self.handled_underrun_frames = underruns;
        if !self.needs_refill() {
            return false;
        }
        if let Some(sink) = self.sink.lock().unwrap().as_ref() {
            sink.pause();
        }
        true
    }

    pub fn time_until_refill(&self) -> Duration {
        let buffered = self.buffered_frames();
        if buffered < self.target_buffer_frames {
            return Duration::ZERO;
        }
        let frames = buffered - self.target_buffer_frames + 1;
        Duration::from_secs_f64(frames as f64 / f64::from(self.sample_rate))
    }

    fn buffered_frames(&self) -> usize {
        self.samples.lock().unwrap().len() + self.staged_frames.load(Ordering::Relaxed)
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
    staged: VecDeque<[i16; 2]>,
    staged_frames: Arc<AtomicUsize>,
    sample_rate: u32,
    last_frame: [i16; 2],
    pending_frame: Option<[i16; 2]>,
    pending_index: u8,
    underrun_frames: Arc<AtomicU64>,
}

impl RingSource {
    fn new(
        samples: Arc<Mutex<VecDeque<[i16; 2]>>>,
        staged_frames: Arc<AtomicUsize>,
        underrun_frames: Arc<AtomicU64>,
        sample_rate: u32,
    ) -> Self {
        Self {
            samples,
            staged: VecDeque::with_capacity(AUDIO_READ_BATCH_FRAMES),
            staged_frames,
            sample_rate,
            last_frame: [0, 0],
            pending_frame: None,
            pending_index: 0,
            underrun_frames,
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

        if self.staged.is_empty() {
            let mut samples = self.samples.lock().unwrap();
            let count = samples.len().min(AUDIO_READ_BATCH_FRAMES);
            self.staged.extend(samples.drain(..count));
            self.staged_frames
                .store(self.staged.len(), Ordering::Relaxed);
        }

        let frame = self.staged.pop_front();
        self.staged_frames
            .store(self.staged.len(), Ordering::Relaxed);
        match frame {
            Some(frame) => {
                self.last_frame = frame;
                self.pending_frame = Some(frame);
                self.pending_index = 1;
                Some(frame[0])
            }
            None => {
                self.underrun_frames.fetch_add(1, Ordering::Relaxed);
                self.last_frame = [
                    (self.last_frame[0] as f32 * 0.985) as i16,
                    (self.last_frame[1] as f32 * 0.985) as i16,
                ];
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::FRAME_INTERVAL_NS;

    #[test]
    fn ring_source_counts_empty_frames_as_underruns() {
        let samples = Arc::new(Mutex::new(VecDeque::from([[100, -100]])));
        let staged = Arc::new(AtomicUsize::new(0));
        let underruns = Arc::new(AtomicU64::new(0));
        let mut source = RingSource::new(samples, staged.clone(), underruns.clone(), 48_000);

        assert_eq!(source.next(), Some(100));
        assert_eq!(staged.load(Ordering::Relaxed), 0);
        assert_eq!(source.next(), Some(-100));
        assert_eq!(underruns.load(Ordering::Relaxed), 0);

        let _ = source.next();
        assert_eq!(underruns.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn target_buffer_covers_more_than_one_game_boy_frame() {
        let target = buffer_frames_for_ms(48_000, TARGET_BUFFER_MS);
        let samples_per_frame = 48_000.0 * 70_224.0 / 4_194_304.0;

        assert!(target as f64 > samples_per_frame);
    }

    #[test]
    fn refill_deadline_waits_until_buffer_reaches_low_water_mark() {
        let output = AudioOutput::new();
        output
            .samples
            .lock()
            .unwrap()
            .resize(output.target_buffer_frames, [0, 0]);

        assert!(output.time_until_refill() > Duration::ZERO);

        output.samples.lock().unwrap().pop_front();
        assert_eq!(output.time_until_refill(), Duration::ZERO);
    }

    #[test]
    fn sustained_frame_pacing_does_not_underrun_audio() {
        const CPU_HZ: u128 = 4_194_304;
        const FRAME_CYCLES: u128 = 70_224;
        const SAMPLE_RATE: u128 = 48_000;
        const ONE_SECOND_NS: u128 = 1_000_000_000;

        let target = buffer_frames_for_ms(SAMPLE_RATE as u32, TARGET_BUFFER_MS) as u128;
        let mut queued = 0_u128;
        let mut emulated_sample_phase = 0_u128;
        while queued < target {
            emulated_sample_phase += FRAME_CYCLES * SAMPLE_RATE;
            queued += emulated_sample_phase / CPU_HZ;
            emulated_sample_phase %= CPU_HZ;
        }

        let mut consumed_sample_phase = 0_u128;
        for _ in 0..(60 * 60) {
            consumed_sample_phase += u128::from(FRAME_INTERVAL_NS) * SAMPLE_RATE;
            let consumed = consumed_sample_phase / ONE_SECOND_NS;
            consumed_sample_phase %= ONE_SECOND_NS;
            assert!(queued >= consumed, "audio queue underrun");
            queued -= consumed;

            emulated_sample_phase += FRAME_CYCLES * SAMPLE_RATE;
            queued += emulated_sample_phase / CPU_HZ;
            emulated_sample_phase %= CPU_HZ;
        }
    }
}
