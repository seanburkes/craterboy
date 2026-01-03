use crate::domain::Emulator;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const APU_SAMPLE_RATE: f64 = 59.7275;
const UPSAMPLE_RATIO: f64 = AUDIO_SAMPLE_RATE as f64 / APU_SAMPLE_RATE;

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
        sink.set_volume(0.5);

        self.stream = Some(stream);
        self.stream_handle = Some(stream_handle);
        *self.sink.lock().unwrap() = Some(sink);
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let sink = self.sink.clone();
        let emulator_ptr = emulator as *mut Emulator as usize;

        thread::spawn(move || {
            let mut buffer = Vec::with_capacity(1024);
            let mut phase: f64 = 0.0;
            let mut prev_sample: f64 = 0.0;
            let mut curr_sample: f64 = 0.0;
            let mut has_curr = false;

            while running.load(Ordering::SeqCst) {
                let emulator = unsafe { &mut *(emulator_ptr as *mut Emulator) };

                if emulator.apu_has_sample() {
                    prev_sample = curr_sample;
                    curr_sample = emulator.apu_take_sample() as f64;
                    has_curr = true;
                }

                while phase < 1.0 {
                    if buffer.len() >= 960 {
                        break;
                    }

                    let interpolated = if has_curr {
                        let t = phase.clamp(0.0, 1.0);
                        prev_sample + (curr_sample - prev_sample) * t
                    } else {
                        prev_sample
                    };

                    let output_sample = interpolated.clamp(-128.0, 127.0) as i16;
                    buffer.push(output_sample);
                    buffer.push(output_sample);

                    phase += 1.0 / UPSAMPLE_RATIO;
                }

                phase -= 1.0;

                if buffer.len() >= 512 {
                    let rodio_samples: Vec<i16> = buffer.drain(..).collect();
                    if let Some(sink_guard) = sink.lock().unwrap().as_ref() {
                        if !sink_guard.empty() {
                            thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                        sink_guard.append(rodio::buffer::SamplesBuffer::new(
                            2,
                            AUDIO_SAMPLE_RATE,
                            rodio_samples,
                        ));
                    } else {
                        thread::sleep(Duration::from_millis(1));
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
