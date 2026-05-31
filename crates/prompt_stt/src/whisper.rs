use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use candle_core::{Device, Tensor, DType};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as m, Config};
use tokenizers::Tokenizer;

const MODEL_REPO: &str = "openai/whisper-base.en";

pub struct WhisperModel {
    pub is_loaded: bool,
    model: Option<m::model::Whisper>,
    tokenizer: Option<Tokenizer>,
    config: Option<Config>,
    mel_filters: Option<Vec<f32>>,
    device: Device,
}

impl WhisperModel {
    pub fn new() -> Result<Self> {
        Ok(Self {
            is_loaded: false,
            model: None,
            tokenizer: None,
            config: None,
            mel_filters: None,
            device: Device::Cpu,
        })
    }

    pub async fn download_model() -> Result<PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
        let models_dir = home_dir.join(".nir").join("models").join("whisper");
        fs::create_dir_all(&models_dir)?;

        let hf_base = format!("https://huggingface.co/{MODEL_REPO}/resolve/main");
        let files: [(&str, String); 4] = [
            ("model.safetensors", format!("{hf_base}/model.safetensors")),
            ("config.json", format!("{hf_base}/config.json")),
            ("tokenizer.json", format!("{hf_base}/tokenizer.json")),
            ("melfilters.bytes", "https://github.com/huggingface/candle/raw/main/candle-examples/examples/whisper/melfilters.bytes".to_string()),
        ];

        // Clean up any corrupted/empty/failed files (e.g. containing "Entry not found")
        for (name, _) in &files {
            let path = models_dir.join(name);
            if path.exists() {
                if let Ok(metadata) = fs::metadata(&path) {
                    if metadata.len() < 100 {
                        log::warn!("File {:?} is too small ({} bytes), removing it for re-download...", path, metadata.len());
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }

        let all_exist = files.iter().all(|(name, _)| models_dir.join(name).exists());
        if all_exist {
            log::info!("Whisper models already exist at {:?}", models_dir);
            return Ok(models_dir);
        }

        log::info!("Downloading {MODEL_REPO} model...");
        
        let models_dir_clone = models_dir.clone();
        let handle = std::thread::spawn(move || -> Result<()> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
                
            rt.block_on(async {
                for (name, url) in files {
                    let path = models_dir_clone.join(name);
                    if !path.exists() {
                        log::info!("Downloading {}...", name);
                        let response = reqwest::get(url).await?.bytes().await?;
                        let mut file = fs::File::create(&path)?;
                        file.write_all(&response)?;
                    }
                }
                Ok(())
            })
        });

        handle.join().map_err(|_| anyhow!("Download thread panicked"))??;

        log::info!("{MODEL_REPO} download complete!");
        Ok(models_dir)
    }

    pub fn load(&mut self, models_dir: &Path) -> Result<()> {
        let device = Device::Cpu;
        
        // 1. Load config
        let config_path = models_dir.join("config.json");
        let config_bytes = fs::read(config_path)?;
        let config: Config = serde_json::from_slice(&config_bytes)?;
        
        // 2. Load tokenizer
        let tokenizer_path = models_dir.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| anyhow!(e))?;
        
        // 3. Load model weights
        let model_path = models_dir.join("model.safetensors");
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[model_path], DType::F32, &device)?
        };
        let model = m::model::Whisper::load(&vb, config.clone())?;
        
        // 4. Load Mel filters
        let mel_bytes = fs::read(models_dir.join("melfilters.bytes"))?;
        let mut mel_filters = vec![0f32; mel_bytes.len() / 4];
        for (i, chunk) in mel_bytes.chunks_exact(4).enumerate() {
            mel_filters[i] = f32::from_le_bytes(chunk.try_into().unwrap());
        }
        
        self.model = Some(model);
        self.tokenizer = Some(tokenizer);
        self.config = Some(config);
        self.mel_filters = Some(mel_filters);
        self.device = device;
        self.is_loaded = true;
        
        log::info!("Whisper model successfully loaded!");
        Ok(())
    }

    pub fn transcribe(&mut self, audio_buffer: &[f32], from_hz: u32, channels: u16) -> Result<String> {
        if !self.is_loaded {
            return Err(anyhow!("Model is not loaded."));
        }
        
        let model = self.model.as_mut().ok_or_else(|| anyhow!("Model not initialized"))?;
        let tokenizer = self.tokenizer.as_ref().ok_or_else(|| anyhow!("Tokenizer not initialized"))?;
        let config = self.config.as_ref().ok_or_else(|| anyhow!("Config not initialized"))?;
        let mel_filters = self.mel_filters.as_ref().ok_or_else(|| anyhow!("Mel filters not initialized"))?;

        log::info!("Preprocessing audio: {} samples at {}Hz, {} channels...", audio_buffer.len(), from_hz, channels);
        
        // 1. Resample and convert to mono (16kHz)
        let processed_audio = resample_to_16k(audio_buffer, from_hz, channels);
        log::info!("Processed audio has {} samples at 16kHz mono", processed_audio.len());

        // 2. Compute Mel spectrogram
        // pcm_to_mel returns a Vec<f32> directly
        let mel = m::audio::pcm_to_mel(config, &processed_audio, mel_filters);
        let mel_len = mel.len();
        let mel = Tensor::from_vec(
            mel,
            (1, config.num_mel_bins, mel_len / config.num_mel_bins),
            &self.device,
        )?;

        // 3. Run Encoder
        log::info!("Running Whisper encoder...");
        let audio_features = model.encoder.forward(&mel, true)?;

        // 4. Token generation loop
        log::info!("Decoding tokens...");
        let sot_token = token_id(tokenizer, m::SOT_TOKEN)?;
        let transcribe_token = token_id(tokenizer, m::TRANSCRIBE_TOKEN)?;
        let notimestamps_token = token_id(tokenizer, m::NO_TIMESTAMPS_TOKEN)?;
        
        let mut tokens = vec![sot_token, transcribe_token, notimestamps_token];
        let eot_token = token_id(tokenizer, m::EOT_TOKEN)?;
        
        let mut token_ids = Vec::new();
        let audio_duration_secs = processed_audio.len() as f32 / 16000.0;
        let max_steps = ((audio_duration_secs * 8.0) as usize).clamp(5, 100);
        for step in 0..max_steps {
            let token_tensor = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let decoder_output = model.decoder.forward(&token_tensor, &audio_features, true)?;
            let logits = model.decoder.final_linear(&decoder_output)?;
            
            // Get logits at last sequence step: shape [1, seq_len, vocab_size] -> narrow it
            let logits = logits.narrow(1, tokens.len() - 1, 1)?.squeeze(0)?.squeeze(0)?;
            
            // Greedy sampling: argmax
            let next_token = logits
                .to_vec1::<f32>()?
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx as u32)
                .ok_or_else(|| anyhow!("Logits empty at step {}", step))?;
                
            if next_token == eot_token {
                break;
            }
            
            token_ids.push(next_token);
            tokens.push(next_token);
        }

        // 5. Decode token IDs into string
        let text = tokenizer.decode(&token_ids, true).map_err(|e| anyhow!(e))?;
        let cleaned_text = text.trim().to_string();
        log::info!("Whisper transcription: '{}'", cleaned_text);
        
        Ok(cleaned_text)
    }
}

fn token_id(tokenizer: &Tokenizer, token: &str) -> Result<u32> {
    match tokenizer.token_to_id(token) {
        None => anyhow::bail!("no token-id for {token}"),
        Some(id) => Ok(id),
    }
}

fn resample_to_16k(input: &[f32], from_hz: u32, channels: u16) -> Vec<f32> {
    // 1. Make mono
    let mono = if channels == 1 {
        input.to_vec()
    } else {
        let ch = channels as usize;
        let mut out = Vec::with_capacity(input.len() / ch);
        for chunk in input.chunks_exact(ch) {
            let sum: f32 = chunk.iter().sum();
            out.push(sum / ch as f32);
        }
        out
    };

    // 2. Resample to 16000Hz using linear interpolation
    if from_hz == 16000 {
        mono
    } else {
        let ratio = from_hz as f64 / 16000.0;
        let new_len = (mono.len() as f64 / ratio).round() as usize;
        let mut output = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = pos - idx as f64;
            if idx + 1 < mono.len() {
                output.push(mono[idx] * (1.0 - frac) as f32 + mono[idx + 1] * frac as f32);
            } else if idx < mono.len() {
                output.push(mono[idx]);
            }
        }
        output
    }
}
