pub mod audio_capture;
pub mod whisper;

use anyhow::Result;
use whisper::WhisperModel;
use audio_capture::AudioCapture;

pub struct SttService {
    whisper: WhisperModel,
    audio_capture: AudioCapture,
}

impl SttService {
    pub fn new() -> Result<Self> {
        Ok(Self {
            whisper: WhisperModel::new()?,
            audio_capture: AudioCapture::new()?,
        })
    }

    pub async fn download_models_if_needed(&mut self) -> Result<()> {
        if !self.whisper.is_loaded {
            let path = WhisperModel::download_model().await?;
            self.whisper.load(&path)?;
        }
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.whisper.is_loaded
    }

    pub fn start_listening(&mut self) -> Result<()> {
        self.audio_capture.start()
    }

    pub fn stop_listening(&mut self) -> Result<String> {
        let sample_rate = self.audio_capture.sample_rate();
        let channels = self.audio_capture.channels();
        let audio_data = self.audio_capture.stop()?;
        self.whisper.transcribe(&audio_data, sample_rate, channels)
    }
}
