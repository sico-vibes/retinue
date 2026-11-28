//! Microphone capture via cpal.
//!
//! Runs the input stream on its own thread (cpal streams are not `Send` on all
//! backends) and delivers raw interleaved frames to a callback. Resampling to
//! 16 kHz mono happens later, before the sherpa-onnx boundary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::{SpeechError, SpeechResult};

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct CaptureSession {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureSession {
    /// Stop capture and wait for the audio thread to release the device.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Build one input stream for one sample type, converting every frame to the
/// `f32` in [-1, 1] the rest of the crate works in. Separate from
/// `start_capture` because the type has to be chosen at the one call that builds
/// the stream, and a desktop's default device is `f32` on Windows but commonly
/// `i16` on ALSA (PulseAudio and PipeWire included).
fn build_typed<T, F>(device: &cpal::Device, config: &cpal::StreamConfig, mut on_frame: F) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
    F: FnMut(AudioFrame) + Send + 'static,
{
    let sample_rate = config.sample_rate.0;
    let channels = config.channels;
    device.build_input_stream(
        config,
        move |data: &[T], _info| {
            let samples = data.iter().map(|sample| sample.to_sample::<f32>()).collect();
            on_frame(AudioFrame { samples, sample_rate, channels });
        },
        |error| eprintln!("Retinue audio capture: {error}"),
        None,
    )
}

/// Start capturing from the default input device. The callback runs on the audio
/// thread, so keep it cheap (send over a channel rather than decoding inline).
pub fn start_capture<F>(on_frame: F) -> SpeechResult<CaptureSession>
where
    F: FnMut(AudioFrame) + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();

    let handle = thread::spawn(move || {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            let _ = ready_tx.send(Err("no default input device is available".into()));
            return;
        };
        let supported = match device.default_input_config() {
            Ok(config) => config,
            Err(error) => {
                let _ = ready_tx.send(Err(error.to_string()));
                return;
            }
        };
        let format = supported.sample_format();
        let stream_config: cpal::StreamConfig = supported.into();
        let stream = match format {
            cpal::SampleFormat::F32 => build_typed::<f32, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::I16 => build_typed::<i16, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::U16 => build_typed::<u16, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::I32 => build_typed::<i32, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::U32 => build_typed::<u32, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::F64 => build_typed::<f64, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::I8 => build_typed::<i8, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::U8 => build_typed::<u8, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::I64 => build_typed::<i64, _>(&device, &stream_config, on_frame),
            cpal::SampleFormat::U64 => build_typed::<u64, _>(&device, &stream_config, on_frame),
            other => {
                let _ = ready_tx.send(Err(format!("unsupported sample format: {other:?}")));
                return;
            }
        };
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                let _ = ready_tx.send(Err(error.to_string()));
                return;
            }
        };
        if let Err(error) = stream.play() {
            let _ = ready_tx.send(Err(error.to_string()));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        while !stop_thread.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(50));
        }
        drop(stream);
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(CaptureSession {
            stop,
            thread: Some(handle),
        }),
        Ok(Err(error)) => Err(SpeechError::Capture(error)),
        Err(_) => Err(SpeechError::Capture("capture thread exited early".into())),
    }
}
