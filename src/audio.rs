use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crate::synth::SynthEngine;

pub struct VisualizerBuffer {
    pub samples: Vec<f32>,
    pub write_ptr: usize,
}

impl VisualizerBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            samples: vec![0.0; size],
            write_ptr: 0,
        }
    }

    pub fn push(&mut self, sample: f32) {
        if self.samples.is_empty() {
            return;
        }
        self.samples[self.write_ptr] = sample;
        self.write_ptr = (self.write_ptr + 1) % self.samples.len();
    }

    pub fn get_samples(&self) -> Vec<f32> {
        // Return samples in correct chronological order
        let mut ordered = vec![0.0; self.samples.len()];
        let len = self.samples.len();
        for i in 0..len {
            ordered[i] = self.samples[(self.write_ptr + i) % len];
        }
        ordered
    }
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    pub synth: Arc<Mutex<SynthEngine>>,
    pub visualizer_buf: Arc<Mutex<VisualizerBuffer>>,
}

impl AudioEngine {
    pub fn new() -> Result<Self, anyhow::Error> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No default audio output device found"))?;

        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0 as f32;

        let synth = Arc::new(Mutex::new(SynthEngine::new(sample_rate)));
        let visualizer_buf = Arc::new(Mutex::new(VisualizerBuffer::new(512)));

        let synth_clone = Arc::clone(&synth);
        let vis_buf_clone = Arc::clone(&visualizer_buf);

        let channels = config.channels() as usize;

        // Callback to process audio frames
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    write_audio(data, channels, &synth_clone, &vis_buf_clone);
                },
                |err| eprintln!("Audio stream error: {:?}", err),
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config.into(),
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    write_audio_i16(data, channels, &synth_clone, &vis_buf_clone);
                },
                |err| eprintln!("Audio stream error: {:?}", err),
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config.into(),
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    write_audio_u16(data, channels, &synth_clone, &vis_buf_clone);
                },
                |err| eprintln!("Audio stream error: {:?}", err),
                None,
            )?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;

        Ok(Self {
            _stream: stream,
            synth,
            visualizer_buf,
        })
    }
}

fn write_audio(
    data: &mut [f32],
    channels: usize,
    synth: &Arc<Mutex<SynthEngine>>,
    vis_buf: &Arc<Mutex<VisualizerBuffer>>,
) {
    let mut synth_locked = synth.lock().unwrap();
    let mut vis_locked = vis_buf.lock().unwrap();

    for frame in data.chunks_mut(channels) {
        let (sample_l, sample_r) = synth_locked.process_next_sample();
        
        if channels >= 2 {
            frame[0] = sample_l;
            frame[1] = sample_r;
            // Pad remaining channels if multi-channel system (5.1, 7.1, etc.)
            for extra in frame.iter_mut().skip(2) {
                *extra = 0.0;
            }
        } else if channels == 1 {
            frame[0] = (sample_l + sample_r) * 0.5;
        }

        // Push mono mix of synthesized audio to visualizer
        vis_locked.push((sample_l + sample_r) * 0.5);
    }
}

fn write_audio_i16(
    data: &mut [i16],
    channels: usize,
    synth: &Arc<Mutex<SynthEngine>>,
    vis_buf: &Arc<Mutex<VisualizerBuffer>>,
) {
    let mut synth_locked = synth.lock().unwrap();
    let mut vis_locked = vis_buf.lock().unwrap();

    for frame in data.chunks_mut(channels) {
        let (sample_l, sample_r) = synth_locked.process_next_sample();
        
        let out_l = (sample_l * i16::MAX as f32) as i16;
        let out_r = (sample_r * i16::MAX as f32) as i16;

        if channels >= 2 {
            frame[0] = out_l;
            frame[1] = out_r;
            for extra in frame.iter_mut().skip(2) {
                *extra = 0;
            }
        } else if channels == 1 {
            frame[0] = (((sample_l + sample_r) * 0.5) * i16::MAX as f32) as i16;
        }

        vis_locked.push((sample_l + sample_r) * 0.5);
    }
}

fn write_audio_u16(
    data: &mut [u16],
    channels: usize,
    synth: &Arc<Mutex<SynthEngine>>,
    vis_buf: &Arc<Mutex<VisualizerBuffer>>,
) {
    let mut synth_locked = synth.lock().unwrap();
    let mut vis_locked = vis_buf.lock().unwrap();

    for frame in data.chunks_mut(channels) {
        let (sample_l, sample_r) = synth_locked.process_next_sample();
        
        // Scale f32 [-1.0, 1.0] to u16 [0, u16::MAX]
        let out_l = (((sample_l + 1.0) * 0.5) * u16::MAX as f32) as u16;
        let out_r = (((sample_r + 1.0) * 0.5) * u16::MAX as f32) as u16;

        if channels >= 2 {
            frame[0] = out_l;
            frame[1] = out_r;
            for extra in frame.iter_mut().skip(2) {
                *extra = u16::MAX / 2;
            }
        } else if channels == 1 {
            let mono = (sample_l + sample_r) * 0.5;
            frame[0] = (((mono + 1.0) * 0.5) * u16::MAX as f32) as u16;
        }

        vis_locked.push((sample_l + sample_r) * 0.5);
    }
}
