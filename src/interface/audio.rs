use crate::domain::Emulator;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const APU_SAMPLE_RATE: f64 = 59.7275;
const UPSAMPLE_RATIO: f64 = AUDIO_SAMPLE_RATE as f64 / APU_SAMPLE_RATE;
const CHUNK_SIZE: usize = 512;
const DEFAULT_VOLUME: f32 = 0.3;
const MAX_QUEUED_CHUNKS: usize = 4;

pub struct AudioOutput {
    stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Arc<Mutex<Option<Sink>>>,
    running: Arc<AtomicBool>,
}

impl AudioOutput {
    pub fn new() -> Self {
        Self {
            stream: None,
            stream_handle: None,
            sink: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&mut self, emulator: &mut Emulator) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        let (stream, stream_handle) = OutputStream::try_default().ok().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.set_volume(DEFAULT_VOLUME);

        self.stream = Some(stream);
        self.stream_handle = Some(stream_handle);
        *self.sink.lock().unwrap() = Some(sink);
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let sink = self.sink.clone();
        let emulator_ptr = emulator as *mut Emulator as usize;

        thread::spawn(move || {
            let mut output_buffer: Vec<i16> = Vec::with_capacity(CHUNK_SIZE * 2);
            let mut input_buffer: Vec<f64> = Vec::with_capacity(256);
            let mut phase: f64 = 0.0;
            let mut prev_sample: f64 = 0.0;
            let mut next_sample: f64 = 0.0;
            let mut has_next: bool = false;

            while running.load(Ordering::SeqCst) {
                let emulator = unsafe { &mut *(emulator_ptr as *mut Emulator) };

                while emulator.apu_has_sample() && input_buffer.len() < 256 {
                    input_buffer.push(emulator.apu_take_sample() as f64);
                }

                if !has_next && !input_buffer.is_empty() {
                    prev_sample = next_sample;
                    next_sample = input_buffer.remove(0);
                    has_next = true;
                    phase = 0.0;
                }

                output_buffer.clear();

                while has_next && phase < 1.0 && output_buffer.len() < CHUNK_SIZE * 2 {
                    let t = phase.clamp(0.0, 1.0);
                    let interpolated = prev_sample + (next_sample - prev_sample) * t;

                    let output_sample = (interpolated * 256.0).clamp(-32768.0, 32767.0) as i16;
                    output_buffer.push(output_sample);
                    output_buffer.push(output_sample);

                    phase += 1.0 / UPSAMPLE_RATIO;
                }

                if phase >= 1.0 {
                    has_next = false;
                }

                if !output_buffer.is_empty() {
                    if let Some(sink_guard) = sink.lock().unwrap().as_ref() {
                        while sink_guard.len() > MAX_QUEUED_CHUNKS {
                            thread::sleep(Duration::from_millis(1));
                            if !running.load(Ordering::SeqCst) {
                                return;
                            }
                        }
                        sink_guard.append(rodio::buffer::SamplesBuffer::new(
                            2,
                            AUDIO_SAMPLE_RATE,
                            output_buffer.clone(),
                        ));
                    }
                } else {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        });
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
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
