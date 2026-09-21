//! Audio shared by BASIC's CPC-style synthesizer and modern sampled sounds.
//!
//! Opening an output device is lazy. Without an output device, the same commands
//! keep their timing and queue semantics, so a sound never freezes a program.

mod backend;
pub mod cpc;
mod shared_cpc;

use backend::{open_output, OutputBackend};
pub use cpc::{Note, Section};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::PlaybackState;
use kira::{AudioManager, Decibels, Frame, Tween};
use shared_cpc::SharedCpc;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const CHANNELS: usize = 32;
const ASSETS: u8 = 64;
const MAX_ASSET_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct RateRamp {
    from: f64,
    to: f64,
    duration: f64,
    elapsed: f64,
}

impl RateRamp {
    fn value(&self) -> f64 {
        self.from + (self.to - self.from) * (self.elapsed / self.duration).min(1.0)
    }

    /// Integral of playback rate over elapsed wall time, for silent playback.
    fn advance(&mut self, seconds: f64) -> f64 {
        let ramp_time = seconds.min((self.duration - self.elapsed).max(0.0));
        let before = self.value();
        self.elapsed += ramp_time;
        let after = self.value();
        ramp_time * (before + after) * 0.5 + (seconds - ramp_time) * self.to
    }
}

struct Voice {
    asset: u8,
    duration: f64,
    position: f64,
    looping: bool,
    paused: bool,
    stopped: bool,
    rate: f64,
    rate_ramp: Option<RateRamp>,
    handle: Option<StaticSoundHandle>,
}

impl Voice {
    fn advance(&mut self, seconds: f64) {
        if self.stopped {
            return;
        }
        // Kira advances parameter transitions even when sample playback is
        // paused. Silent playback follows the same clock for rate changes.
        let delta = if let Some(ramp) = &mut self.rate_ramp {
            let delta = ramp.advance(seconds);
            if ramp.elapsed >= ramp.duration {
                self.rate = ramp.to;
                self.rate_ramp = None;
            }
            delta
        } else {
            seconds * self.rate
        };
        if let Some(handle) = &self.handle {
            self.position = handle.position();
            self.stopped = handle.state() == PlaybackState::Stopped;
            return;
        }
        if self.paused {
            return;
        }
        self.position += delta;
        if self.looping {
            self.position %= self.duration;
        } else if self.position >= self.duration {
            self.position = self.duration;
            self.stopped = true;
        }
    }

    fn stop(&mut self) {
        if let Some(handle) = &mut self.handle {
            handle.stop(instant_tween());
        }
        self.stopped = true;
    }
}

/// Interpreter-owned audio state. All public controls are called on its thread;
/// only the small CPC synthesizer is shared with the output callback.
pub struct AudioSystem {
    enabled: bool,
    engaged: bool,
    attempted: bool,
    last_error: String,
    manager: Option<AudioManager<OutputBackend>>,
    synth: Arc<SharedCpc>,
    assets: BTreeMap<u8, StaticSoundData>,
    voices: Vec<Option<Voice>>,
    last_tick: Instant,
}

impl fmt::Debug for AudioSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioSystem")
            .field("enabled", &self.enabled)
            .field("available", &self.manager.is_some())
            .field("last_error", &self.last_error)
            .field("loaded_assets", &self.assets.len())
            .finish()
    }
}

impl Default for AudioSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioSystem {
    pub fn new() -> Self {
        let enabled = std::env::var("AVL_BASIC_AUDIO")
            .map(|value| {
                !matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "off" | "0" | "false"
                )
            })
            .unwrap_or(true);
        Self::with_enabled(enabled)
    }

    /// Deterministic headless mode, also useful for tests without environment changes.
    #[cfg(test)]
    pub fn new_disabled() -> Self {
        Self::with_enabled(false)
    }

    fn with_enabled(enabled: bool) -> Self {
        Self {
            enabled,
            engaged: false,
            attempted: false,
            last_error: if enabled {
                String::new()
            } else {
                "Audio output is disabled".to_string()
            },
            manager: None,
            synth: Arc::new(SharedCpc::default()),
            assets: BTreeMap::new(),
            voices: (0..CHANNELS).map(|_| None).collect(),
            last_tick: Instant::now(),
        }
    }

    fn ensure_output(&mut self) {
        if !self.enabled || self.attempted {
            return;
        }
        self.attempted = true;
        match open_output() {
            Ok(mut manager) => match manager.play(self.synth.begin_output()) {
                Ok(()) => {
                    self.manager = Some(manager);
                    self.last_error.clear();
                }
                Err(error) => {
                    self.synth.end_output();
                    self.last_error = error.to_string();
                }
            },
            Err(error) => self.last_error = error,
        }
        // Device setup may take time. It is not elapsed playback time for the
        // first note, and must not be applied again by the silent clock.
        self.last_tick = Instant::now();
    }

    fn engage(&mut self) {
        if !self.engaged {
            self.last_tick = Instant::now();
            self.engaged = true;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.tick();
        if !enabled {
            self.stop_all();
            self.last_error = "Audio output is disabled".to_string();
        } else if self.manager.is_none() {
            // Explicit AUDIO ON also permits retrying after a device failure.
            self.last_error.clear();
        }
        self.enabled = enabled;
        self.attempted = self.manager.is_some();
        self.last_tick = Instant::now();
    }

    pub fn available(&mut self) -> bool {
        self.engage();
        self.tick();
        self.ensure_output();
        self.manager.is_some()
    }

    pub fn error(&mut self) -> String {
        // An explicit diagnostic query also checks a device that has gone idle.
        self.engage();
        self.tick();
        self.last_error.clone()
    }

    pub fn tick(&mut self) {
        // The interpreter polls frequently. Ordinary programs must not pay for
        // clock reads, locks, or voice scans if they never use sound.
        if !self.engaged {
            return;
        }
        let now = Instant::now();
        let seconds = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        self.advance(seconds);
    }

    fn advance(&mut self, seconds: f64) {
        let backend_error = self.manager.as_mut().and_then(|m| m.backend_mut().error());
        if let Some(error) = backend_error {
            self.fail_output(error);
            // The renderer has already accounted for part of this interval.
            // Start the silent clock here instead of advancing that part twice.
            return;
        }
        if self.manager.is_none() {
            self.synth.advance(seconds);
        }
        for voice in self.voices.iter_mut().flatten() {
            voice.advance(seconds);
        }
        if !self
            .voices
            .iter()
            .flatten()
            .any(|voice| !voice.stopped && (!voice.paused || voice.rate_ramp.is_some()))
        {
            self.engaged = !self.synth.is_idle();
        }
    }

    fn fail_output(&mut self, error: String) {
        self.last_error = error;
        for voice in self.voices.iter_mut().flatten() {
            voice.stop();
        }
        // Invalidate the old renderer before retiring its device. A delayed
        // callback must never consume notes now owned by the silent clock.
        self.synth.end_output();
        self.manager = None;
    }

    pub fn enqueue(&mut self, note: Note) -> bool {
        self.engage();
        self.tick();
        self.engaged = true;
        self.ensure_output();
        self.synth.enqueue(note)
    }

    pub fn define_volume(&mut self, id: u8, sections: Vec<Section>) {
        self.tick();
        self.synth.define_volume(id, sections);
    }

    pub fn define_tone(&mut self, id: u8, repeating: bool, sections: Vec<Section>) {
        self.tick();
        self.synth.define_tone(id, repeating, sections);
    }

    pub fn release(&mut self, mask: u8) {
        self.engage();
        self.tick();
        self.synth.release(mask);
    }

    pub fn sq(&mut self, channel: u8) -> u8 {
        self.tick();
        self.synth.status(channel)
    }

    /// Stops playback while retaining loaded samples and envelope definitions.
    pub fn stop_all(&mut self) {
        self.stop(None);
        self.synth.end_output();
        self.manager = None;
        self.synth.stop();
        // Ready must not retain a device or a failed opening attempt. The next
        // program opens a fresh output when it first needs audio.
        self.attempted = false;
        self.engaged = false;
    }

    /// Program reset: settings survive, while sounds, assets and envelopes do not.
    pub fn reset(&mut self) {
        self.stop_all();
        self.synth.clear();
        self.assets.clear();
        self.engaged = false;
        self.last_tick = Instant::now();
    }

    pub fn load(&mut self, id: u8, path: &Path) -> Result<(), String> {
        check_asset(id)?;
        if std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_ASSET_BYTES
        {
            return Err("Audio file exceeds the 128 MiB limit".to_string());
        }
        let data = StaticSoundData::from_file(path).map_err(|error| error.to_string())?;
        if data.sample_rate == 0 || data.num_frames() == 0 {
            return Err("The audio file contains no samples".to_string());
        }
        // Kira decodes before this check; this is an asset/cache policy, not a
        // claim that arbitrary compressed input cannot allocate more temporarily.
        let other_frames: usize = self
            .assets
            .iter()
            .filter(|(key, _)| **key != id)
            .map(|(_, asset)| asset.num_frames())
            .sum();
        if data.num_frames().saturating_add(other_frames)
            > MAX_ASSET_BYTES as usize / std::mem::size_of::<Frame>()
        {
            return Err("Decoded audio cache exceeds the 128 MiB limit".to_string());
        }
        // Replace only after successful decoding. Existing voices retain their
        // own shared sample data until they finish or are stopped.
        self.assets.insert(id, data);
        Ok(())
    }

    pub fn unload(&mut self, id: Option<u8>) {
        self.tick();
        for slot in &mut self.voices {
            if slot
                .as_ref()
                .is_some_and(|voice| id.is_none() || id == Some(voice.asset))
            {
                if let Some(voice) = slot {
                    voice.stop();
                }
                *slot = None;
            }
        }
        match id {
            Some(id) => {
                self.assets.remove(&id);
            }
            None => self.assets.clear(),
        }
    }

    pub fn play(
        &mut self,
        channel: u8,
        id: u8,
        looping: bool,
        volume: f64,
        pan: f64,
        rate: f64,
    ) -> Result<(), String> {
        let index = channel_index(channel)?;
        check_asset(id)?;
        check_volume(volume)?;
        check_pan(pan)?;
        check_rate(rate)?;
        let mut data = self
            .assets
            .get(&id)
            .ok_or_else(|| format!("Sound {id} is not loaded"))?
            .clone();
        let duration = data.duration().as_secs_f64();
        data = data
            .volume(decibels(volume))
            .panning(pan as f32)
            .playback_rate(rate);
        if looping {
            data = data.loop_region(..);
        }
        self.engage();
        self.tick();
        self.ensure_output();
        let handle = match &mut self.manager {
            Some(manager) => Some(manager.play(data).map_err(|error| error.to_string())?),
            None => None,
        };
        if let Some(old) = &mut self.voices[index] {
            old.stop();
        }
        self.voices[index] = Some(Voice {
            asset: id,
            duration,
            position: 0.0,
            looping,
            paused: false,
            stopped: false,
            rate,
            rate_ramp: None,
            handle,
        });
        self.engaged = true;
        Ok(())
    }

    pub fn stop(&mut self, channel: Option<u8>) {
        self.tick();
        for (index, slot) in self.voices.iter_mut().enumerate() {
            if channel.is_none() || channel == Some(index as u8 + 1) {
                if let Some(voice) = slot {
                    voice.stop();
                }
                *slot = None;
            }
        }
    }

    pub fn pause(&mut self, channel: Option<u8>) {
        self.tick();
        for (index, voice) in self.voices.iter_mut().enumerate() {
            if channel.is_none() || channel == Some(index as u8 + 1) {
                if let Some(voice) = voice {
                    if !voice.stopped {
                        voice.paused = true;
                        if let Some(handle) = &mut voice.handle {
                            handle.pause(instant_tween());
                        }
                    }
                }
            }
        }
    }

    pub fn resume(&mut self, channel: Option<u8>) {
        self.engage();
        self.tick();
        for (index, voice) in self.voices.iter_mut().enumerate() {
            if channel.is_none() || channel == Some(index as u8 + 1) {
                if let Some(voice) = voice {
                    if !voice.stopped {
                        voice.paused = false;
                        self.engaged = true;
                        if let Some(handle) = &mut voice.handle {
                            handle.resume(instant_tween());
                        }
                    }
                }
            }
        }
    }

    pub fn set_volume(&mut self, channel: u8, value: f64, fade_ms: u64) -> Result<(), String> {
        let index = channel_index(channel)?;
        check_volume(value)?;
        self.tick();
        if let Some(handle) = self.voices[index].as_mut().and_then(|v| v.handle.as_mut()) {
            handle.set_volume(decibels(value), tween(fade_ms));
        }
        Ok(())
    }

    pub fn set_pan(&mut self, channel: u8, value: f64, fade_ms: u64) -> Result<(), String> {
        let index = channel_index(channel)?;
        check_pan(value)?;
        self.tick();
        if let Some(handle) = self.voices[index].as_mut().and_then(|v| v.handle.as_mut()) {
            handle.set_panning(value as f32, tween(fade_ms));
        }
        Ok(())
    }

    pub fn set_rate(&mut self, channel: u8, value: f64, fade_ms: u64) -> Result<(), String> {
        let index = channel_index(channel)?;
        check_rate(value)?;
        self.engage();
        self.tick();
        if let Some(voice) = &mut self.voices[index] {
            if let Some(handle) = &mut voice.handle {
                handle.set_playback_rate(value, tween(fade_ms));
            }
            let current = voice.rate_ramp.as_ref().map_or(voice.rate, RateRamp::value);
            if fade_ms == 0 {
                voice.rate = value;
                voice.rate_ramp = None;
            } else {
                voice.rate_ramp = Some(RateRamp {
                    from: current,
                    to: value,
                    duration: fade_ms as f64 / 1000.0,
                    elapsed: 0.0,
                });
                if !voice.stopped {
                    self.engaged = true;
                }
            }
        }
        Ok(())
    }

    pub fn state(&mut self, channel: u8) -> i32 {
        self.engage();
        self.tick();
        let Ok(index) = channel_index(channel) else {
            return 0;
        };
        match &self.voices[index] {
            Some(voice) if !voice.stopped => {
                if voice.paused {
                    2
                } else {
                    1
                }
            }
            _ => 0,
        }
    }

    pub fn position(&mut self, channel: u8) -> f64 {
        self.engage();
        self.tick();
        channel_index(channel)
            .ok()
            .and_then(|index| self.voices[index].as_ref())
            .map_or(0.0, |voice| voice.position)
    }
}

fn channel_index(channel: u8) -> Result<usize, String> {
    if (1..=CHANNELS as u8).contains(&channel) {
        Ok(channel as usize - 1)
    } else {
        Err("Sound channel must be between 1 and 32".to_string())
    }
}

fn check_asset(id: u8) -> Result<(), String> {
    if (1..=ASSETS).contains(&id) {
        Ok(())
    } else {
        Err("Sound ID must be between 1 and 64".to_string())
    }
}

fn check_volume(value: f64) -> Result<(), String> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err("Sound volume must be between 0 and 1".to_string())
    }
}

fn check_pan(value: f64) -> Result<(), String> {
    if value.is_finite() && (-1.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err("Sound pan must be between -1 and 1".to_string())
    }
}

fn check_rate(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err("Sound rate must be positive and finite".to_string())
    }
}

fn decibels(amplitude: f64) -> Decibels {
    if amplitude <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels((20.0 * amplitude.log10()) as f32)
    }
}

fn tween(fade_ms: u64) -> Tween {
    Tween {
        duration: Duration::from_millis(fade_ms),
        ..Default::default()
    }
}

fn instant_tween() -> Tween {
    tween(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_file(seconds: u32) -> tempfile::NamedTempFile {
        use std::io::Write;
        let sample_rate = 8000_u32;
        let bytes = sample_rate * seconds * 2;
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend((bytes + 36).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(sample_rate.to_le_bytes());
        wav.extend((sample_rate * 2).to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(bytes.to_le_bytes());
        wav.resize(44 + bytes as usize, 0);
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&wav).unwrap();
        file
    }

    #[test]
    fn silent_playback_preserves_pause_loop_rate_and_stop_semantics() {
        let file = wav_file(1);
        let mut audio = AudioSystem::new_disabled();
        audio.load(1, file.path()).unwrap();
        audio.play(1, 1, true, 1.0, 0.0, 1.0).unwrap();
        audio.advance(0.25);
        assert!((audio.position(1) - 0.25).abs() < 0.01);
        audio.pause(Some(1));
        audio.advance(10.0);
        assert_eq!(audio.state(1), 2);
        assert!((audio.position(1) - 0.25).abs() < 0.01);
        audio.resume(Some(1));
        audio.set_rate(1, 2.0, 0).unwrap();
        audio.advance(0.5);
        assert!((audio.position(1) - 0.25).abs() < 0.02);
        audio.stop(Some(1));
        assert_eq!(audio.state(1), 0);
        assert_eq!(audio.position(1), 0.0);
        assert!(!audio.available());
    }

    #[test]
    fn silent_nonlooping_sound_finishes_and_rate_fade_integrates_time() {
        let file = wav_file(2);
        let mut audio = AudioSystem::new_disabled();
        audio.load(1, file.path()).unwrap();
        audio.play(1, 1, false, 1.0, 0.0, 1.0).unwrap();
        audio.set_rate(1, 3.0, 1000).unwrap();
        audio.advance(0.5);
        assert!((audio.position(1) - 0.75).abs() < 0.02);
        audio.advance(0.6);
        assert_eq!(audio.state(1), 0);
        assert_eq!(audio.position(1), 2.0);
    }

    #[test]
    fn silent_rate_transition_continues_while_playback_is_paused() {
        let file = wav_file(4);
        let mut audio = AudioSystem::new_disabled();
        audio.load(1, file.path()).unwrap();
        audio.play(1, 1, false, 1.0, 0.0, 1.0).unwrap();
        audio.pause(Some(1));
        audio.set_rate(1, 3.0, 1000).unwrap();
        audio.advance(1.25);
        assert_eq!(audio.state(1), 2);
        assert!(audio.position(1) < 0.01);
        audio.resume(Some(1));
        audio.advance(0.5);
        assert!((audio.position(1) - 1.5).abs() < 0.02);
    }

    #[test]
    fn failed_load_preserves_cache_and_unload_stops_its_voices() {
        let file = wav_file(1);
        let mut audio = AudioSystem::new_disabled();
        audio.load(1, file.path()).unwrap();
        assert!(audio
            .load(1, Path::new("missing-audio-test-file.wav"))
            .is_err());
        audio.play(2, 1, true, 1.0, 0.0, 1.0).unwrap();
        audio.unload(Some(1));
        assert_eq!(audio.state(2), 0);
        assert!(audio.play(2, 1, false, 1.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn stop_all_preserves_assets_but_reset_unloads_them() {
        let file = wav_file(1);
        let mut audio = AudioSystem::new_disabled();
        audio.load(1, file.path()).unwrap();
        audio.play(1, 1, false, 1.0, 0.0, 1.0).unwrap();
        audio.stop_all();
        assert_eq!(audio.state(1), 0);
        audio.play(1, 1, false, 1.0, 0.0, 1.0).unwrap();
        audio.reset();
        assert_eq!(audio.state(1), 0);
        assert!(audio.play(1, 1, false, 1.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn a_new_run_can_retry_an_earlier_device_failure() {
        let mut audio = AudioSystem::new_disabled();
        audio.attempted = true;
        audio.last_error = "Output was unavailable".to_string();
        audio.reset();
        assert!(!audio.attempted);
        // Keep the diagnostic until a later opening succeeds.
        assert_eq!(audio.error(), "Output was unavailable");
        audio.attempted = true;
        audio.stop_all();
        assert!(!audio.attempted);
        assert!(!audio.engaged);
    }

    #[test]
    fn failed_output_leaves_cpc_queues_advancing_on_the_silent_clock() {
        let mut audio = AudioSystem::new_disabled();
        let note = Note {
            state: 1,
            period: 284,
            duration: 10,
            volume: 12,
            volume_env: 0,
            tone_env: 0,
            noise: 0,
        };
        assert!(audio.enqueue(note));
        assert!(audio.enqueue(note));
        assert_ne!(audio.sq(1), 4);
        audio.fail_output("Audio output stopped responding".to_string());
        audio.advance(0.3);
        assert_eq!(audio.sq(1), 4);
        assert!(!audio.engaged);
        assert_eq!(audio.error(), "Audio output stopped responding");
        audio.set_enabled(true);
        assert!(!audio.attempted);
        assert!(audio.error().is_empty());
    }

    #[test]
    fn invalid_ids_and_nonfinite_controls_do_not_panic() {
        let mut audio = AudioSystem::new_disabled();
        assert!(audio.set_volume(0, 0.5, 0).is_err());
        assert!(audio.set_pan(33, 0.0, 0).is_err());
        assert!(audio.set_rate(1, f64::NAN, 0).is_err());
        assert_eq!(audio.state(255), 0);
        assert_eq!(audio.position(0), 0.0);
        audio.pause(Some(255));
        audio.resume(Some(255));
        audio.stop(Some(255));
    }
}
