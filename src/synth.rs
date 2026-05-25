use std::f32::consts::PI;
use serde::{Serialize, Deserialize};
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Waveform {
    Sine,
    Square,
    Saw,
    Triangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InstrumentType {
    Subtractive,
    Fm,
    Plucked,
    Drum,
    Additive,
    Supersaw,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AdsrConfig {
    pub attack: f32,   // seconds
    pub decay: f32,    // seconds
    pub sustain: f32,  // level (0.0 - 1.0)
    pub release: f32,  // seconds
}

impl Default for AdsrConfig {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.15,
            sustain: 0.7,
            release: 0.3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone)]
pub struct AdsrEnvelope {
    pub config: AdsrConfig,
    pub state: EnvelopeState,
    pub current_value: f32,
    pub sample_rate: f32,
    pub time_in_state: f32,
    pub release_start_value: f32,
}

impl AdsrEnvelope {
    pub fn new(config: AdsrConfig, sample_rate: f32) -> Self {
        Self {
            config,
            state: EnvelopeState::Idle,
            current_value: 0.0,
            sample_rate,
            time_in_state: 0.0,
            release_start_value: 0.0,
        }
    }

    pub fn trigger_on(&mut self) {
        self.state = EnvelopeState::Attack;
        self.time_in_state = 0.0;
    }

    pub fn trigger_off(&mut self) {
        self.state = EnvelopeState::Release;
        self.time_in_state = 0.0;
        self.release_start_value = self.current_value;
    }

    pub fn is_idle(&self) -> bool {
        self.state == EnvelopeState::Idle
    }

    pub fn tick(&mut self) -> f32 {
        let dt = 1.0 / self.sample_rate;
        self.time_in_state += dt;

        match self.state {
            EnvelopeState::Idle => {
                self.current_value = 0.0;
            }
            EnvelopeState::Attack => {
                if self.config.attack <= 0.0 {
                    self.current_value = 1.0;
                    self.state = EnvelopeState::Decay;
                    self.time_in_state = 0.0;
                } else {
                    let progress = self.time_in_state / self.config.attack;
                    if progress >= 1.0 {
                        self.current_value = 1.0;
                        self.state = EnvelopeState::Decay;
                        self.time_in_state = 0.0;
                    } else {
                        self.current_value = progress;
                    }
                }
            }
            EnvelopeState::Decay => {
                if self.config.decay <= 0.0 {
                    self.current_value = self.config.sustain;
                    self.state = EnvelopeState::Sustain;
                    self.time_in_state = 0.0;
                } else {
                    let progress = self.time_in_state / self.config.decay;
                    if progress >= 1.0 {
                        self.current_value = self.config.sustain;
                        self.state = EnvelopeState::Sustain;
                        self.time_in_state = 0.0;
                    } else {
                        self.current_value = 1.0 - (1.0 - self.config.sustain) * progress;
                    }
                }
            }
            EnvelopeState::Sustain => {
                self.current_value = self.config.sustain;
            }
            EnvelopeState::Release => {
                if self.config.release <= 0.0 {
                    self.current_value = 0.0;
                    self.state = EnvelopeState::Idle;
                    self.time_in_state = 0.0;
                } else {
                    let progress = self.time_in_state / self.config.release;
                    if progress >= 1.0 {
                        self.current_value = 0.0;
                        self.state = EnvelopeState::Idle;
                        self.time_in_state = 0.0;
                    } else {
                        self.current_value = self.release_start_value * (1.0 - progress);
                    }
                }
            }
        }

        self.current_value
    }
}

// Biquad Lowpass filter state
#[derive(Debug, Clone, Default)]
pub struct LowPassFilter {
    pub cutoff: f32,
    pub resonance: f32,
    pub sample_rate: f32,
    // Coefficients
    a1: f32,
    a2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    // History
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl LowPassFilter {
    pub fn new(cutoff: f32, resonance: f32, sample_rate: f32) -> Self {
        let mut filter = Self {
            cutoff: cutoff.clamp(20.0, sample_rate / 2.1),
            resonance: resonance.clamp(0.01, 10.0),
            sample_rate,
            ..Default::default()
        };
        filter.calculate_coefficients();
        filter
    }

    pub fn update(&mut self, cutoff: f32, resonance: f32) {
        self.cutoff = cutoff.clamp(20.0, self.sample_rate / 2.1);
        self.resonance = resonance.clamp(0.01, 10.0);
        self.calculate_coefficients();
    }

    fn calculate_coefficients(&mut self) {
        // Standard biquad lowpass filter calculations
        let w0 = 2.0 * PI * self.cutoff / self.sample_rate;
        let alpha = w0.sin() / (2.0 * self.resonance);
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 - cos_w0) / 2.0) / a0;
        self.b1 = (1.0 - cos_w0) / a0;
        self.b2 = ((1.0 - cos_w0) / 2.0) / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;

        y
    }
}

#[derive(Debug, Clone)]
pub struct Voice {
    pub track_idx: usize,
    pub note: u8,
    pub velocity: f32,
    pub phase: f32,
    pub phase_step: f32,
    
    // FM synth modulator properties
    pub fm_phase: f32,
    pub fm_phase_step: f32,
    pub fm_index: f32,
    pub fm_ratio: f32,

    // Plucked string (Karplus-Strong) properties
    pub plucked_buffer: Vec<f32>,
    pub plucked_ptr: usize,
    pub plucked_last_value: f32,

    // Drum synthesizer properties
    pub drum_type: Option<DrumType>,
    pub drum_ticks: usize,

    pub envelope: AdsrEnvelope,
    pub instrument_type: InstrumentType,
    pub subtractive_waveform: Waveform,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrumType {
    Kick,
    Snare,
    HiHat,
}

impl Voice {
    pub fn new(
        track_idx: usize,
        note: u8,
        velocity: f32,
        instrument_type: InstrumentType,
        subtractive_waveform: Waveform,
        adsr_config: AdsrConfig,
        fm_ratio: f32,
        fm_index: f32,
        sample_rate: f32,
    ) -> Self {
        let freq = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let phase_step = freq * 2.0 * PI / sample_rate;

        // FM Modulator step
        let fm_mod_freq = freq * fm_ratio;
        let fm_phase_step = fm_mod_freq * 2.0 * PI / sample_rate;

        // Karplus-Strong string pluck initialization
        let plucked_buffer = if instrument_type == InstrumentType::Plucked {
            let buffer_size = (sample_rate / freq).round() as usize;
            let mut rng = rand::thread_rng();
            let mut buf = vec![0.0; buffer_size];
            for val in buf.iter_mut() {
                *val = rng.gen_range(-1.0..1.0);
            }
            buf
        } else {
            Vec::new()
        };

        // Drum trigger logic mapped to standard MIDI notes:
        // C2 (36) = Kick, D2 (38) = Snare, F#2 (42) = Hi-Hat
        let drum_type = if instrument_type == InstrumentType::Drum {
            match note {
                36 => Some(DrumType::Kick),
                38 => Some(DrumType::Snare),
                42 => Some(DrumType::HiHat),
                _ => {
                    // Fall back based on octave mapping
                    let rem = note % 12;
                    if rem == 0 || rem == 1 || rem == 2 || rem == 3 || rem == 4 {
                        Some(DrumType::Kick)
                    } else if rem == 5 || rem == 6 || rem == 7 || rem == 8 || rem == 9 {
                        Some(DrumType::Snare)
                    } else {
                        Some(DrumType::HiHat)
                    }
                }
            }
        } else {
            None
        };

        let mut envelope = AdsrEnvelope::new(adsr_config, sample_rate);
        envelope.trigger_on();

        Self {
            track_idx,
            note,
            velocity,
            phase: 0.0,
            phase_step,
            fm_phase: 0.0,
            fm_phase_step,
            fm_index,
            fm_ratio,
            plucked_buffer,
            plucked_ptr: 0,
            plucked_last_value: 0.0,
            drum_type,
            drum_ticks: 0,
            envelope,
            instrument_type,
            subtractive_waveform,
        }
    }

    pub fn trigger_off(&mut self) {
        self.envelope.trigger_off();
    }

    pub fn tick(&mut self) -> f32 {
        let env_amp = self.envelope.tick();
        if self.envelope.is_idle() {
            return 0.0;
        }

        let raw_sample = match self.instrument_type {
            InstrumentType::Subtractive => self.tick_subtractive(),
            InstrumentType::Fm => self.tick_fm(),
            InstrumentType::Plucked => self.tick_plucked(),
            InstrumentType::Drum => self.tick_drum(),
            InstrumentType::Additive => self.tick_additive(),
            InstrumentType::Supersaw => self.tick_supersaw(),
        };

        raw_sample * env_amp * self.velocity
    }

    fn tick_subtractive(&mut self) -> f32 {
        let sample = match self.subtractive_waveform {
            Waveform::Sine => self.phase.sin(),
            Waveform::Square => {
                if self.phase < PI {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Saw => 2.0 * (self.phase / (2.0 * PI)) - 1.0,
            Waveform::Triangle => {
                let norm = self.phase / (2.0 * PI);
                2.0 * (2.0 * norm - 1.0).abs() - 1.0
            }
        };

        self.phase = (self.phase + self.phase_step) % (2.0 * PI);
        sample
    }

    fn tick_fm(&mut self) -> f32 {
        // Modulator modulates carrier phase
        let mod_sample = self.fm_phase.sin() * self.fm_index;
        let carrier_phase = (self.phase + mod_sample) % (2.0 * PI);
        let sample = carrier_phase.sin();

        self.phase = (self.phase + self.phase_step) % (2.0 * PI);
        self.fm_phase = (self.fm_phase + self.fm_phase_step) % (2.0 * PI);

        sample
    }

    fn tick_plucked(&mut self) -> f32 {
        if self.plucked_buffer.is_empty() {
            return 0.0;
        }

        let ptr = self.plucked_ptr;
        let val = self.plucked_buffer[ptr];

        // Lowpass feedback filter (averaged with last sample)
        let decay = 0.995; // feedback decay coefficient
        let next_val = 0.5 * (val + self.plucked_last_value) * decay;

        self.plucked_buffer[ptr] = next_val;
        self.plucked_last_value = val;
        self.plucked_ptr = (ptr + 1) % self.plucked_buffer.len();

        val
    }

    fn tick_drum(&mut self) -> f32 {
        let sample_rate = self.envelope.sample_rate;
        let t = self.drum_ticks as f32 / sample_rate;
        self.drum_ticks += 1;

        let drum_type = match self.drum_type {
            Some(dt) => dt,
            None => return 0.0,
        };

        match drum_type {
            DrumType::Kick => {
                // Rapid pitch sweep: starts at 180Hz and sweeps down to 48Hz very fast
                let pitch_decay = (-45.0 * t).exp();
                let kick_freq = 48.0 + (180.0 - 48.0) * pitch_decay;
                
                let step = kick_freq * 2.0 * PI / sample_rate;
                self.phase = (self.phase + step) % (2.0 * PI);
                
                // Fundamental sine wave body
                let body = self.phase.sin();
                
                // Attack click: short transient burst of noise
                let click_env = (-120.0 * t).exp();
                let click = if click_env > 0.01 {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(-1.0..1.0) * click_env * 0.25
                } else {
                    0.0
                };

                let amp_env = (-8.0 * t).exp();
                let raw_kick = (body + click) * amp_env;
                
                // Apply warm saturation (soft-clipping) for a thick, premium analog thump!
                raw_kick.tanh() * 1.2
            }
            DrumType::Snare => {
                // Head tone: sweeps from 180Hz to 120Hz
                let pitch_decay = (-35.0 * t).exp();
                let snare_freq = 120.0 + (180.0 - 120.0) * pitch_decay;
                
                let step = snare_freq * 2.0 * PI / sample_rate;
                self.phase = (self.phase + step) % (2.0 * PI);
                
                let tone_env = (-15.0 * t).exp();
                let tone = self.phase.sin() * tone_env * 0.35;

                // Snare wires (High-passed noise to remove low rumble and make it crisp and snappy)
                let noise_env = (-7.0 * t).exp();
                if noise_env > 0.001 {
                    let mut rng = rand::thread_rng();
                    let raw_noise = rng.gen_range(-1.0..1.0);
                    let hp_noise = raw_noise - self.plucked_last_value;
                    self.plucked_last_value = raw_noise;
                    
                    let noise = hp_noise * noise_env * 0.45;
                    (tone + noise).tanh() * 1.0
                } else {
                    tone.tanh() * 1.0
                }
            }
            DrumType::HiHat => {
                // Short, snappy envelope for a clean closed hat
                let hat_env = (-65.0 * t).exp();
                if hat_env < 0.001 {
                    return 0.0;
                }

                let mut rng = rand::thread_rng();
                let raw_noise = rng.gen_range(-1.0..1.0);

                // High-pass filter (difference) to keep only the crisp high-frequencies
                let hp_noise = raw_noise - self.plucked_last_value;
                self.plucked_last_value = raw_noise;

                // Emulate metallic resonance by mixing a high-frequency component
                let metal_freq = 8000.0;
                let step = metal_freq * 2.0 * PI / sample_rate;
                self.phase = (self.phase + step) % (2.0 * PI);
                let metallic_component = self.phase.sin() * 0.2;

                ((hp_noise + metallic_component) * hat_env * 0.65).tanh()
            }
        }
    }

    fn tick_additive(&mut self) -> f32 {
        // Accumulate up to 6 harmonics: 1st (fundamental), 2nd, 3rd, 4th, 5th, 6th
        let mut sample = 0.0;
        let amp = 0.4;
        let phase_base = self.phase;
        
        // fm_ratio will act as custom harmonic spacing (default scale)
        // fm_index will act as richness decay morphing (between 0.0 and 20.0)
        // High index: slow harmonic roll-off (bright organ/bell)
        // Low index: steep harmonic roll-off (warm organ/sine)
        let decay_exponent = 1.8 - (self.fm_index * 0.06).clamp(0.0, 1.5);
        let harmonic_count = 6;
        for i in 1..=harmonic_count {
            let harmonic_scale = i as f32 * self.fm_ratio * 0.5; // custom scale spacing
            let h_phase = (phase_base * harmonic_scale) % (2.0 * PI);
            let harmonic_amp = amp / (i as f32).powf(decay_exponent);
            sample += h_phase.sin() * harmonic_amp;
        }

        self.phase = (self.phase + self.phase_step) % (2.0 * PI);
        sample * 0.5
    }

    fn tick_supersaw(&mut self) -> f32 {
        // Synthesizes detuned stacked sawtooth oscillators
        // fm_ratio acts as detuning spread (0.0 to 1.0)
        // fm_index acts as side-oscillator count (2 to 6, indicating total 3 to 7 stacked waves)
        let detune = self.fm_ratio * 0.08; // scale spread to reasonable musical range
        let num_oscillators = 1 + (self.fm_index.round() as usize).clamp(2, 6);
        let mut sample = 0.0;

        for i in 0..num_oscillators {
            let coeff = if num_oscillators > 1 {
                (i as f32 / (num_oscillators - 1) as f32) - 0.5
            } else {
                0.0
            };

            // Calculate individual phase step scale
            let osc_phase = (self.phase * (1.0 + coeff * detune)) % (2.0 * PI);
            let osc_saw = 2.0 * (osc_phase / (2.0 * PI)) - 1.0;
            sample += osc_saw;
        }

        self.phase = (self.phase + self.phase_step) % (2.0 * PI);
        sample / (num_oscillators as f32).sqrt() * 0.35 // normalize and adjust level
    }
}

// Struct to store synthesis settings for each of the 4 tracks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentConfig {
    pub name: String,
    pub inst_type: InstrumentType,
    pub waveform: Waveform,
    pub adsr: AdsrConfig,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub volume: f32,  // 0.0 - 1.0
    pub pan: f32,     // -1.0 (L) to 1.0 (R)
    pub mute: bool,
    pub solo: bool,
}

impl InstrumentConfig {
    pub fn new(name: &str, inst_type: InstrumentType) -> Self {
        let adsr = match inst_type {
            InstrumentType::Subtractive => AdsrConfig::default(),
            InstrumentType::Fm => AdsrConfig {
                attack: 0.005,
                decay: 0.25,
                sustain: 0.4,
                release: 0.4,
            },
            InstrumentType::Plucked => AdsrConfig {
                attack: 0.001,
                decay: 0.8,
                sustain: 0.0,
                release: 0.5,
            },
            InstrumentType::Drum => AdsrConfig {
                attack: 0.001,
                decay: 0.3,
                sustain: 0.0,
                release: 0.1,
            },
            InstrumentType::Additive => AdsrConfig {
                attack: 0.05,
                decay: 0.2,
                sustain: 0.8,
                release: 0.4,
            },
            InstrumentType::Supersaw => AdsrConfig {
                attack: 0.01,
                decay: 0.20,
                sustain: 0.6,
                release: 0.4,
            },
        };

        let (fm_ratio, fm_index) = match inst_type {
            InstrumentType::Supersaw => (0.15, 4.0), // Detuning spread (15%) and 4 side-oscillators (5 stacked waves)
            InstrumentType::Additive => (1.0, 5.0),  // Default spacing and richness
            _ => (2.0, 3.5),
        };

        Self {
            name: name.to_string(),
            inst_type,
            waveform: Waveform::Saw,
            adsr,
            fm_ratio,
            fm_index,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }
}

pub struct SynthEngine {
    pub voices: Vec<Voice>,
    pub sample_rate: f32,
    
    // Stereo Feedback Delay
    pub delay_enabled: bool,
    pub delay_time: f32,       // seconds
    pub delay_feedback: f32,   // 0.0 - 0.95
    pub delay_buffer_l: Vec<f32>,
    pub delay_buffer_r: Vec<f32>,
    pub delay_ptr: usize,

    // Resonant Lowpass Filter
    pub filter_enabled: bool,
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_l: LowPassFilter,
    pub filter_r: LowPassFilter,

    // Instrument configurations for the 4 tracks
    pub instruments: Vec<InstrumentConfig>,
}

impl SynthEngine {
    pub fn new(sample_rate: f32) -> Self {
        // Initializing stereo delay buffers (max 2 seconds of delay)
        let max_delay_samples = (sample_rate * 2.0) as usize;

        // Tracks: Track 1 (Lead), Track 2 (Bass), Track 3 (Pads), Track 4 (Drums)
        let instruments = vec![
            InstrumentConfig::new("Lead Synth", InstrumentType::Subtractive),
            InstrumentConfig::new("FM Pluck", InstrumentType::Fm),
            InstrumentConfig::new("Harp/Guitar", InstrumentType::Plucked),
            InstrumentConfig::new("Drum Synth", InstrumentType::Drum),
        ];

        Self {
            voices: Vec::new(),
            sample_rate,
            delay_enabled: true,
            delay_time: 0.3,
            delay_feedback: 0.35,
            delay_buffer_l: vec![0.0; max_delay_samples],
            delay_buffer_r: vec![0.0; max_delay_samples],
            delay_ptr: 0,
            filter_enabled: true,
            filter_cutoff: 2000.0,
            filter_resonance: 1.0,
            filter_l: LowPassFilter::new(2000.0, 1.0, sample_rate),
            filter_r: LowPassFilter::new(2000.0, 1.0, sample_rate),
            instruments,
        }
    }

    pub fn note_on(&mut self, track_idx: usize, note: u8, velocity: f32) {
        if track_idx >= self.instruments.len() {
            return;
        }

        // Clean any existing voice playing the exact same note on this track
        self.note_off(track_idx, note);

        let inst = &self.instruments[track_idx];
        let voice = Voice::new(
            track_idx,
            note,
            velocity * inst.volume,
            inst.inst_type,
            inst.waveform,
            inst.adsr,
            inst.fm_ratio,
            inst.fm_index,
            self.sample_rate,
        );

        self.voices.push(voice);
    }

    pub fn note_off(&mut self, _track_idx: usize, note: u8) {
        // Set all matching note voices to release state
        for voice in self.voices.iter_mut() {
            if voice.note == note {
                voice.trigger_off();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for voice in self.voices.iter_mut() {
            voice.trigger_off();
        }
    }

    pub fn set_waveform(&mut self, track_idx: usize, waveform: Waveform) {
        if track_idx < self.instruments.len() {
            self.instruments[track_idx].waveform = waveform;
        }
    }

    pub fn update_adsr(&mut self, track_idx: usize, adsr: AdsrConfig) {
        if track_idx < self.instruments.len() {
            self.instruments[track_idx].adsr = adsr;
        }
    }

    pub fn update_filter(&mut self, cutoff: f32, resonance: f32) {
        self.filter_cutoff = cutoff;
        self.filter_resonance = resonance;
        self.filter_l.update(cutoff, resonance);
        self.filter_r.update(cutoff, resonance);
    }

    pub fn process_next_sample(&mut self) -> (f32, f32) {
        let mut mix_l = 0.0;
        let mut mix_r = 0.0;

        // Is there any solo track?
        let has_solo = self.instruments.iter().any(|inst| inst.solo);

        // Mix all active voices
        // To route voices to their respective track, we match voice's settings
        // Wait, standard midi voice is triggered per track, so when creating a voice
        // let's record which track it belongs to. In Voice struct let's make track_idx optional,
        // or we can search voices. Let's add a track_idx field to Voice!
        // Oh wait, we didn't add track_idx to Voice, but we can easily filter them or update our Voice struct.
        // Actually, we can check matching types or simply let all voices mix.
        // To be accurate, let's edit the process loop or we can add track_idx to voice.
        // Let's modify Voice to have a `track_idx` field so the mixer knows what volume/pan/mute/solo to apply!
        
        let mut active_voices = std::mem::take(&mut self.voices);
        for voice in active_voices.iter_mut() {
            let track_idx = voice.track_idx;
            if track_idx >= self.instruments.len() {
                continue;
            }
            let inst = &self.instruments[track_idx];

            let sample = voice.tick();

            // Apply Track volume, Mute, Solo
            let should_play = if has_solo {
                inst.solo && !inst.mute
            } else {
                !inst.mute
            };

            if should_play {
                // Apply stereo panning: pan is -1.0 (L) to 1.0 (R)
                let pan_r = (inst.pan + 1.0) / 2.0;
                let pan_l = 1.0 - pan_r;

                mix_l += sample * pan_l;
                mix_r += sample * pan_r;
            }
        }

        // Restore voices that are still active (not idle)
        self.voices = active_voices.into_iter().filter(|v| !v.envelope.is_idle()).collect();

        // 1. Resonant Lowpass Filter Effect
        if self.filter_enabled {
            mix_l = self.filter_l.process(mix_l);
            mix_r = self.filter_r.process(mix_r);
        }

        // 2. Stereo Feedback Delay Effect
        if self.delay_enabled {
            let delay_samples = (self.delay_time * self.sample_rate).round() as usize;
            let delay_size = self.delay_buffer_l.len();
            
            if delay_samples > 0 {
                let read_idx = (self.delay_ptr + delay_size - delay_samples) % delay_size;
                let delayed_l = self.delay_buffer_l[read_idx];
                let delayed_r = self.delay_buffer_r[read_idx];

                // Write feedback to delay buffer (mix dry input and feedback)
                self.delay_buffer_l[self.delay_ptr] = mix_l + delayed_r * self.delay_feedback; // Ping-pong delay!
                self.delay_buffer_r[self.delay_ptr] = mix_r + delayed_l * self.delay_feedback;

                self.delay_ptr = (self.delay_ptr + 1) % delay_size;

                // Mix dry signals with wet delay signals
                mix_l += delayed_l * 0.4;
                mix_r += delayed_r * 0.4;
            }
        }

        (mix_l, mix_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frequency_conversion() {
        // Test standard middle C (note 60) -> ~261.63 Hz
        let freq_c4 = 440.0 * 2.0f32.powf((60.0 - 69.0) / 12.0);
        assert!((freq_c4 - 261.63).abs() < 0.1);

        // Test standard concert pitch A4 (note 69) -> 440.0 Hz
        let freq_a4 = 440.0 * 2.0f32.powf((69.0 - 69.0) / 12.0);
        assert_eq!(freq_a4, 440.0);
    }

    #[test]
    fn test_adsr_envelope() {
        let config = AdsrConfig {
            attack: 0.1,
            decay: 0.1,
            sustain: 0.5,
            release: 0.2,
        };
        let mut env = AdsrEnvelope::new(config, 44100.0);

        // Initially in idle state
        assert_eq!(env.state, EnvelopeState::Idle);
        assert_eq!(env.current_value, 0.0);

        // Trigger note_on
        env.trigger_on();
        assert_eq!(env.state, EnvelopeState::Attack);

        // Process some ticks to transition
        env.tick(); // should increment value
        assert!(env.current_value > 0.0);
    }

    #[test]
    fn test_synth_engine_new() {
        let engine = SynthEngine::new(44100.0);
        assert_eq!(engine.sample_rate, 44100.0);
        assert_eq!(engine.delay_enabled, true);
        assert_eq!(engine.filter_enabled, true);
        assert_eq!(engine.instruments.len(), 4);
        assert_eq!(engine.instruments[0].name, "Lead Synth");
    }

    #[test]
    fn test_voice_allocator() {
        let mut engine = SynthEngine::new(44100.0);
        
        // Ensure no active voices initially
        assert_eq!(engine.voices.len(), 0);

        // Trigger note-on on Track 0
        engine.note_on(0, 60, 0.8);
        assert_eq!(engine.voices.len(), 1);
        assert_eq!(engine.voices[0].note, 60);
        assert_eq!(engine.voices[0].track_idx, 0);

        // Trigger note-off on Track 0
        engine.note_off(0, 60);
        // Note: the voice will transition to Release phase, so it is still active until released!
        assert_eq!(engine.voices.len(), 1);
        assert_eq!(engine.voices[0].envelope.state, EnvelopeState::Release);
    }
}
