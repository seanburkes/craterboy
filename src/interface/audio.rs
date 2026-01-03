use crate::domain::Emulator;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const CHUNK_SIZE: usize = 1024;
const DEFAULT_VOLUME: f32 = 0.3;
const MAX_QUEUED_CHUNKS: usize = 6;
const MAX_BUFFERED_FRAMES: usize = AUDIO_SAMPLE_RATE as usize / 2;
const VISUALIZER_SAMPLE_WINDOW: usize = 512;
const VISUALIZER_MIN_FREQ: f32 = 80.0;
const VISUALIZER_MAX_FREQ: f32 = 8_000.0;

pub struct AudioOutput {
    stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    running: Arc<AtomicBool>,
    sample_rate: u32,
    samples: Arc<Mutex<VecDeque<[i16; 2]>>>,
    visualizer_samples: Arc<Mutex<VecDeque<i16>>>,
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
            visualizer_samples: Arc::new(Mutex::new(VecDeque::new())),
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
            let mut last_frame: [i16; 2] = [0, 0];

            while running.load(Ordering::SeqCst) {
                output_buffer.clear();
                {
                    let mut queue = samples.lock().unwrap();
                    let count = queue.len().min(CHUNK_SIZE);
                    for _ in 0..count {
                        if let Some(frame) = queue.pop_front() {
                            last_frame = frame;
                            output_buffer.push(frame[0]);
                            output_buffer.push(frame[1]);
                        }
                    }
                }

                let filled = output_buffer.len() / 2;
                if filled < CHUNK_SIZE {
                    let remaining = CHUNK_SIZE - filled;
                    for _ in 0..remaining {
                        output_buffer.push(last_frame[0]);
                        output_buffer.push(last_frame[1]);
                    }
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

        if drained.len() > MAX_BUFFERED_FRAMES {
            drained.drain(0..drained.len() - MAX_BUFFERED_FRAMES);
        }

        let mut mono_frames = Vec::with_capacity(drained.len());
        for frame in drained.iter() {
            let mono = ((frame[0] as i32 + frame[1] as i32) / 2) as i16;
            mono_frames.push(mono);
        }

        {
            let mut queue = self.samples.lock().unwrap();
            while queue.len() + drained.len() > MAX_BUFFERED_FRAMES {
                queue.pop_front();
            }
            queue.extend(drained);
        }

        let mut viz = self.visualizer_samples.lock().unwrap();
        for mono in mono_frames {
            viz.push_back(mono);
        }
        while viz.len() > VISUALIZER_SAMPLE_WINDOW {
            viz.pop_front();
        }
    }

    pub fn visualizer_bars(&self, bands: usize) -> Vec<f32> {
        if bands == 0 {
            return Vec::new();
        }
        let sample_rate = self.sample_rate as f32;
        if sample_rate <= 0.0 {
            return vec![0.0; bands];
        }

        let samples: Vec<f32> = {
            let viz = self.visualizer_samples.lock().unwrap();
            if viz.len() < VISUALIZER_SAMPLE_WINDOW {
                return vec![0.0; bands];
            }
            viz.iter().map(|&s| s as f32 / 32768.0).collect()
        };

        let windowed: Vec<f32> = samples
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let t = i as f32 / (samples.len().saturating_sub(1) as f32);
                let w = 0.5 - 0.5 * (2.0 * PI * t).cos();
                s * w
            })
            .collect();

        let min_freq = VISUALIZER_MIN_FREQ;
        let max_freq = VISUALIZER_MAX_FREQ.min(sample_rate * 0.45);
        let mut bars = Vec::with_capacity(bands);
        let span = (max_freq / min_freq).max(1.0);
        for i in 0..bands {
            let t = if bands == 1 {
                0.0
            } else {
                i as f32 / (bands - 1) as f32
            };
            let freq = min_freq * span.powf(t);
            let power = goertzel(&windowed, freq, sample_rate);
            let amp = (power.sqrt() * 8.0 / windowed.len() as f32)
                .min(1.0)
                .powf(0.6);
            bars.push(amp);
        }
        bars
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.samples.lock().unwrap().clear();
        self.visualizer_samples.lock().unwrap().clear();
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

fn goertzel(samples: &[f32], freq: f32, sample_rate: f32) -> f32 {
    if samples.is_empty() || freq <= 0.0 || sample_rate <= 0.0 {
        return 0.0;
    }
    let n = samples.len() as f32;
    let k = (0.5 + (n * freq / sample_rate)).floor();
    let w = 2.0 * PI * k / n;
    let cosine = w.cos();
    let sine = w.sin();
    let coeff = 2.0 * cosine;
    let mut q1 = 0.0;
    let mut q2 = 0.0;
    for &sample in samples {
        let q0 = coeff * q1 - q2 + sample;
        q2 = q1;
        q1 = q0;
    }
    let real = q1 - q2 * cosine;
    let imag = q2 * sine;
    real * real + imag * imag
}
