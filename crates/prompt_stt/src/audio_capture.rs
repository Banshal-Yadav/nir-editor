use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    pub buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        Ok(Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 0,
            channels: 0,
        })
    }
    
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
    
    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow!("No default input device found"))?;
            
        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        
        self.sample_rate = sample_rate;
        self.channels = channels;
        
        let buffer_clone = Arc::clone(&self.buffer);
        self.buffer.lock().unwrap().clear();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _: &_| {
                        // In a real app we'd resample to 16kHz and convert to mono here
                        // For now we just push the raw data
                        let mut buf = buffer_clone.lock().unwrap();
                        buf.extend_from_slice(data);
                    },
                    move |err| {
                        eprintln!("Audio input error: {}", err);
                    },
                    None
                )?
            },
            // Handle other formats (i16, u16) via match here...
            _ => return Err(anyhow!("Unsupported sample format")),
        };

        stream.play()?;
        self.stream = Some(stream);

        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<Vec<f32>> {
        if let Some(stream) = self.stream.take() {
            stream.pause()?; // Pause the stream
        }
        
        // Extract the buffer
        let data = self.buffer.lock().unwrap().clone();
        Ok(data)
    }
}
