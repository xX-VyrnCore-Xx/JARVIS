//! Cattura del microfono su Windows (WASAPI tramite `cpal`) per l'ascolto «Hey Jarvis» (ADR-0140).
//!
//! Il flusso audio vive nel thread di ascolto: i campioni vengono portati a 16 kHz mono e tenuti solo in
//! memoria nel buffer circolare.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, Stream};

use crate::client::ApiClient;
use crate::wake::{self, Pacing, Phase, Resampler, SampleBuffer, Shared, Transport};

struct Core(Arc<ApiClient>);

impl Transport for Core {
    fn post_audio(&self, path: &str, wav: &[u8]) -> Result<serde_json::Value, String> {
        self.0.post_audio(path, wav)
    }

    fn stop_voice(&self) {
        let _ = self.0.post("/api/voice/stop", None);
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<SampleBuffer>,
    shared: Arc<Shared>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    let mut resampler = Resampler::new(config.sample_rate.0, config.channels);
    let mut converted = Vec::new();
    let mut floats: Vec<f32> = Vec::new();
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            floats.clear();
            floats.extend(data.iter().map(|&s| s.to_sample::<f32>()));
            converted.clear();
            resampler.push(&floats, &mut converted);
            buffer.append(&converted);
        },
        move |error| shared.set_detail(format!("Microfono: {error}")),
        None,
    )
}

fn open(buffer: Arc<SampleBuffer>, shared: Arc<Shared>) -> Result<Stream, String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("Nessun microfono trovato. Collegane uno e controlla i permessi di Windows.")?;
    let supported = device
        .default_input_config()
        .map_err(|error| format!("Microfono non disponibile: {error}"))?;
    let config: cpal::StreamConfig = supported.clone().into();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, buffer, shared),
        SampleFormat::I16 => build::<i16>(&device, &config, buffer, shared),
        SampleFormat::U16 => build::<u16>(&device, &config, buffer, shared),
        SampleFormat::I32 => build::<i32>(&device, &config, buffer, shared),
        other => return Err(format!("Formato audio non supportato: {other:?}")),
    }
    .map_err(|error| format!("Microfono non disponibile: {error}"))?;
    stream
        .play()
        .map_err(|error| format!("Microfono non disponibile: {error}"))?;
    Ok(stream)
}

/// Avvia l'ascolto in un thread dedicato. `on_command` riceve il testo di ogni comando pronunciato.
pub fn start(
    client: Arc<ApiClient>,
    shared: Arc<Shared>,
    on_command: impl FnMut(String) + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut on_command = on_command;
        let buffer = Arc::new(SampleBuffer::default());
        let stream = match open(buffer.clone(), shared.clone()) {
            Ok(stream) => stream,
            Err(message) => {
                shared.set_detail(message);
                shared.set_phase(Phase::Unavailable);
                return;
            }
        };
        wake::run(
            &buffer,
            &shared,
            &Core(client),
            Pacing::default(),
            &mut on_command,
        );
        drop(stream);
    });
}
