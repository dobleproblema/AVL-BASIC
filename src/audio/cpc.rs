//! Device-independent CPC-style sound queues and software envelopes.
//!
//! This is an approximate AY timbre, not cycle-accurate chip emulation. All three
//! channels share one noise generator, but software envelopes belong to each
//! voice. Definitions are copied when a queued note starts. Rendering never
//! allocates; the only dynamically sized input is an envelope definition.

const CHANNELS: usize = 3;
const QUEUE_CAPACITY: usize = 4;
const MAX_SECTIONS: usize = 5;
const CENTISECOND: f64 = 0.01;
const EPSILON: f64 = 1.0e-10;
const TONE_CLOCK: f64 = 62_500.0;
// CPCBasic's musical response: amplitude = (level / 15)^2. This is a
// perceptual approximation, not a calibrated AY DAC transfer function.
const LEVELS: [f32; 16] = [
    0.0,
    1.0 / 225.0,
    4.0 / 225.0,
    9.0 / 225.0,
    16.0 / 225.0,
    25.0 / 225.0,
    36.0 / 225.0,
    49.0 / 225.0,
    64.0 / 225.0,
    81.0 / 225.0,
    100.0 / 225.0,
    121.0 / 225.0,
    144.0 / 225.0,
    169.0 / 225.0,
    196.0 / 225.0,
    1.0,
];

#[derive(Clone, Copy, Debug)]
pub struct Note {
    pub state: u8,
    pub period: u16,
    pub duration: i16,
    pub volume: u8,
    pub volume_env: u8,
    pub tone_env: u8,
    pub noise: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum Section {
    Step { steps: u8, delta: i16, ticks: u16 },
    Absolute { value: u16, ticks: u16 },
}

impl Section {
    fn ticks(self) -> u16 {
        let ticks = match self {
            Self::Step { ticks, .. } | Self::Absolute { ticks, .. } => ticks,
        };
        if ticks == 0 {
            256
        } else {
            ticks
        }
    }

    fn steps(self) -> u16 {
        match self {
            Self::Step { steps, .. } => u16::from(steps).max(1),
            Self::Absolute { .. } => 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Envelope {
    sections: [Section; MAX_SECTIONS],
    len: usize,
    repeat: bool,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            sections: [Section::Absolute { value: 0, ticks: 1 }; MAX_SECTIONS],
            len: 0,
            repeat: false,
        }
    }
}

impl Envelope {
    fn new(sections: Vec<Section>, repeat: bool) -> Self {
        let mut envelope = Self {
            repeat,
            ..Self::default()
        };
        for section in sections.into_iter().take(MAX_SECTIONS) {
            envelope.sections[envelope.len] = section;
            envelope.len += 1;
        }
        envelope
    }

    fn duration(self) -> f64 {
        if self.len == 0 {
            // The default volume envelope is constant for two seconds.
            return 2.0;
        }
        self.sections[..self.len]
            .iter()
            .map(|section| f64::from(section.steps()) * f64::from(section.ticks()) * CENTISECOND)
            .sum()
    }
}

#[derive(Clone, Copy, Debug)]
enum EnvelopeKind {
    Volume,
    Tone,
}

#[derive(Clone, Copy, Debug)]
struct EnvelopeCursor {
    envelope: Envelope,
    section: usize,
    step: u16,
    wait: f64,
    // Zero means unlimited repetitions (only used for repeating tone envelopes).
    cycles_left: u32,
    done: bool,
}

impl EnvelopeCursor {
    fn new(envelope: Envelope, cycles: u32) -> Self {
        Self {
            envelope,
            section: 0,
            step: 0,
            wait: 0.0,
            cycles_left: cycles,
            done: envelope.len == 0,
        }
    }

    fn next_event(self) -> f64 {
        if self.done {
            f64::INFINITY
        } else {
            self.wait.max(0.0)
        }
    }

    fn elapse(&mut self, seconds: f64) {
        if !self.done {
            self.wait -= seconds;
        }
    }

    fn apply(&mut self, mut value: u16, initial: u16, kind: EnvelopeKind) -> u16 {
        if self.done || self.wait > EPSILON {
            return value;
        }
        loop {
            if self.section == self.envelope.len {
                if self.cycles_left == 1 {
                    self.done = true;
                    return value;
                }
                if self.cycles_left > 1 {
                    self.cycles_left -= 1;
                }
                self.section = 0;
                self.step = 0;
                // Volume repetitions restart the note's initial amplitude;
                // repeating tone envelopes deliberately keep their last period.
                if matches!(kind, EnvelopeKind::Volume) {
                    value = initial;
                }
            }
            let section = self.envelope.sections[self.section];
            if self.step == section.steps() {
                self.section += 1;
                self.step = 0;
                continue;
            }
            value = match (section, kind) {
                (Section::Absolute { value, .. }, EnvelopeKind::Volume) => value & 15,
                (Section::Absolute { value, .. }, EnvelopeKind::Tone) => value.min(4095),
                (
                    Section::Step {
                        steps: 0, delta, ..
                    },
                    EnvelopeKind::Volume,
                ) => i32::from(delta).rem_euclid(16) as u16,
                (Section::Step { steps: 0, .. }, EnvelopeKind::Tone) => value,
                (Section::Step { delta, .. }, EnvelopeKind::Volume) => {
                    (i32::from(value) + i32::from(delta)).rem_euclid(16) as u16
                }
                (Section::Step { delta, .. }, EnvelopeKind::Tone) => {
                    (i32::from(value) + i32::from(delta)).clamp(0, 4095) as u16
                }
            };
            self.step += 1;
            self.wait = f64::from(section.ticks()) * CENTISECOND;
            return value;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ActiveNote {
    period: u16,
    volume: u16,
    initial_volume: u16,
    noise: bool,
    phase: f64,
    remaining: f64,
    volume_cursor: EnvelopeCursor,
    tone_cursor: EnvelopeCursor,
}

impl ActiveNote {
    fn new(note: Note, volume: Envelope, tone: Envelope) -> Self {
        let cycles = if note.duration < 0 {
            u32::from(note.duration.unsigned_abs())
        } else {
            1
        };
        let remaining = if note.duration > 0 {
            f64::from(note.duration) * CENTISECOND
        } else {
            volume.duration() * f64::from(cycles)
        };
        let mut active = Self {
            period: note.period.min(4095),
            volume: u16::from(note.volume & 15),
            initial_volume: u16::from(note.volume & 15),
            noise: note.noise != 0,
            phase: 0.0,
            remaining,
            volume_cursor: EnvelopeCursor::new(volume, cycles),
            tone_cursor: EnvelopeCursor::new(tone, if tone.repeat { 0 } else { 1 }),
        };
        active.apply_envelopes();
        active
    }

    fn apply_envelopes(&mut self) {
        self.volume =
            self.volume_cursor
                .apply(self.volume, self.initial_volume, EnvelopeKind::Volume);
        self.period = self
            .tone_cursor
            .apply(self.period, self.period, EnvelopeKind::Tone);
    }

    fn next_event(self) -> f64 {
        self.remaining
            .min(self.volume_cursor.next_event())
            .min(self.tone_cursor.next_event())
            .max(0.0)
    }

    fn elapse(&mut self, seconds: f64) {
        if self.period != 0 {
            self.phase = (self.phase + seconds * TONE_CLOCK / f64::from(self.period)).fract();
        }
        self.remaining -= seconds;
        self.volume_cursor.elapse(seconds);
        self.tone_cursor.elapse(seconds);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Channel {
    pending: [Option<Note>; QUEUE_CAPACITY],
    head: usize,
    len: usize,
    active: Option<ActiveNote>,
}

impl Channel {
    fn first(&self) -> Option<Note> {
        self.pending[self.head]
    }

    fn push(&mut self, note: Note) {
        let tail = (self.head + self.len) % QUEUE_CAPACITY;
        self.pending[tail] = Some(note);
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Note> {
        let note = self.pending[self.head].take()?;
        self.head = (self.head + 1) % QUEUE_CAPACITY;
        self.len -= 1;
        Some(note)
    }
}

#[derive(Debug)]
pub struct CpcSynth {
    channels: [Channel; CHANNELS],
    volume_envelopes: [Envelope; 16],
    tone_envelopes: [Envelope; 16],
    noise_period: u8,
    noise_phase: f64,
    noise_state: u32,
}

impl Default for CpcSynth {
    fn default() -> Self {
        Self {
            channels: [Channel::default(); CHANNELS],
            volume_envelopes: [Envelope::default(); 16],
            tone_envelopes: [Envelope::default(); 16],
            noise_period: 0,
            noise_phase: 0.0,
            noise_state: 0x1ffff,
        }
    }
}

impl CpcSynth {
    /// Atomically queues a note on every selected channel, or changes nothing.
    pub fn enqueue(&mut self, note: Note) -> bool {
        let selected = note.state & 7;
        if selected == 0 {
            return true;
        }
        let flush = note.state & 128 != 0;
        if !flush
            && self.channels.iter().enumerate().any(|(index, channel)| {
                selected & (1 << index) != 0 && channel.len == QUEUE_CAPACITY
            })
        {
            return false;
        }
        for (index, channel) in self.channels.iter_mut().enumerate() {
            if selected & (1 << index) == 0 {
                continue;
            }
            if flush {
                *channel = Channel::default();
            }
            channel.push(note);
        }
        self.start_ready();
        true
    }

    pub fn define_volume(&mut self, index: u8, sections: Vec<Section>) {
        if let Some(envelope) = self.volume_envelopes.get_mut(usize::from(index)) {
            *envelope = Envelope::new(sections, false);
        }
    }

    pub fn define_tone(&mut self, index: u8, repeat: bool, sections: Vec<Section>) {
        if let Some(envelope) = self.tone_envelopes.get_mut(usize::from(index)) {
            *envelope = Envelope::new(sections, repeat);
        }
    }

    /// Releases only notes already waiting at an idle channel's queue head.
    pub fn release(&mut self, mask: u8) {
        for (index, channel) in self.channels.iter_mut().enumerate() {
            if mask & (1 << index) != 0 && channel.active.is_none() {
                if let Some(note) = channel.pending[channel.head].as_mut() {
                    note.state &= !64;
                }
            }
        }
        self.start_ready();
    }

    pub fn status(&self, channel: u8) -> u8 {
        let index = match channel {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => return 0,
        };
        let channel = &self.channels[index];
        let free = (QUEUE_CAPACITY - channel.len) as u8;
        if channel.active.is_some() {
            return free | 128;
        }
        match channel.first() {
            Some(note) if note.state & 64 != 0 => free | 64,
            Some(note) => free | (Self::rendezvous(note, index) << 3),
            None => free,
        }
    }

    pub fn is_idle(&self) -> bool {
        self.channels
            .iter()
            .all(|channel| channel.active.is_none() && channel.len == 0)
    }

    /// Stops voices and pending notes while preserving ENV/ENT definitions.
    pub fn stop(&mut self) {
        self.channels = [Channel::default(); CHANNELS];
        self.noise_period = 0;
        self.noise_phase = 0.0;
        self.noise_state = 0x1ffff;
    }

    /// Resets both sound state and envelope definitions.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn render_frame(&mut self, dt: f64) -> [f32; 2] {
        let mut output = [0.0; 2];
        let noise = if self.noise_state & 1 != 0 { 1.0 } else { -1.0 };
        for (index, channel) in self.channels.iter().enumerate() {
            let Some(active) = channel.active else {
                continue;
            };
            let tone = if active.period == 0 {
                0.0
            } else {
                square_wave(active.phase, TONE_CLOCK * dt / f64::from(active.period))
            };
            let signal = match (active.period != 0, active.noise) {
                (true, true) => (tone + 1.0) * (noise + 1.0) * 0.5 - 0.5,
                (true, false) => tone,
                (false, true) => noise,
                (false, false) => 0.0,
            };
            let sample = signal * LEVELS[usize::from(active.volume)] / 3.0;
            // A is left, C is right, B is centered at equal stereo power.
            // This matches CPCBasic's center-channel downmix to stereo.
            let sample = if index == 1 {
                sample * std::f32::consts::FRAC_1_SQRT_2
            } else {
                sample
            };
            if index != 2 {
                output[0] += sample;
            }
            if index != 0 {
                output[1] += sample;
            }
        }
        self.advance(dt);
        output
    }

    /// Advances by envelope/note boundaries rather than generating silent frames.
    pub fn advance(&mut self, mut seconds: f64) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        while seconds > EPSILON {
            let next = self
                .channels
                .iter()
                .filter_map(|channel| channel.active)
                .map(ActiveNote::next_event)
                .fold(f64::INFINITY, f64::min);
            if !next.is_finite() {
                break;
            }
            let elapsed = seconds.min(next);
            if self.noise_period != 0
                && self
                    .channels
                    .iter()
                    .any(|channel| channel.active.is_some_and(|active| active.noise))
            {
                let phase = self.noise_phase + elapsed * TONE_CLOCK / f64::from(self.noise_period);
                self.noise_state = advance_noise(self.noise_state, phase.floor() as u64);
                self.noise_phase = phase.fract();
            }
            for channel in &mut self.channels {
                if let Some(active) = channel.active.as_mut() {
                    active.elapse(elapsed);
                    if active.remaining <= EPSILON {
                        channel.active = None;
                    } else {
                        active.apply_envelopes();
                    }
                }
            }
            seconds -= elapsed;
            self.start_ready();
        }
    }

    fn rendezvous(note: Note, index: usize) -> u8 {
        (((note.state >> 3) & 7) | (note.state & 7)) & !(1 << index)
    }

    fn start_ready(&mut self) {
        // At most three starts can occur: all sounds have a positive duration.
        for index in 0..CHANNELS {
            if self.channels[index].active.is_some() {
                continue;
            }
            let Some(note) = self.channels[index].first() else {
                continue;
            };
            if note.state & 64 != 0 {
                continue;
            }
            let mut group = 1 << index;
            let mut valid = true;
            loop {
                let previous = group;
                for member in 0..CHANNELS {
                    if group & (1 << member) == 0 {
                        continue;
                    }
                    let channel = &self.channels[member];
                    let Some(head) = channel.first() else {
                        valid = false;
                        break;
                    };
                    if channel.active.is_some() || head.state & 64 != 0 {
                        valid = false;
                        break;
                    }
                    let required = Self::rendezvous(head, member);
                    for target in 0..CHANNELS {
                        if required & (1 << target) == 0 {
                            continue;
                        }
                        let Some(partner) = self.channels[target].first() else {
                            valid = false;
                            break;
                        };
                        if Self::rendezvous(partner, target) & (1 << member) == 0 {
                            valid = false;
                            break;
                        }
                    }
                    if !valid {
                        break;
                    }
                    group |= required;
                }
                if !valid || group == previous {
                    break;
                }
            }
            if !valid {
                continue;
            }
            for member in 0..CHANNELS {
                if group & (1 << member) == 0 {
                    continue;
                }
                let Some(note) = self.channels[member].pop() else {
                    continue;
                };
                let volume = self
                    .volume_envelopes
                    .get(usize::from(note.volume_env))
                    .copied()
                    .unwrap_or_default();
                let tone = self
                    .tone_envelopes
                    .get(usize::from(note.tone_env))
                    .copied()
                    .unwrap_or_default();
                if note.noise != 0 {
                    self.noise_period = note.noise.min(31);
                }
                self.channels[member].active = Some(ActiveNote::new(note, volume, tone));
            }
        }
    }
}

// PolyBLEP edges reduce the aliases of a naively sampled square wave. Tones at
// or above Nyquist are suppressed rather than folded back into audible pitches.
fn square_wave(phase: f64, step: f64) -> f32 {
    if !step.is_finite() || step >= 0.5 || step <= 0.0 {
        return 0.0;
    }
    let square = if phase < 0.5 { 1.0 } else { -1.0 };
    (square + poly_blep(phase, step) - poly_blep((phase + 0.5).fract(), step)) as f32
}

fn poly_blep(phase: f64, step: f64) -> f64 {
    if phase < step {
        let t = phase / step;
        2.0 * t - t * t - 1.0
    } else if phase > 1.0 - step {
        let t = (phase - 1.0) / step;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

const fn noise_step(state: u32) -> u32 {
    (state >> 1) | (((state ^ (state >> 3)) & 1) << 16)
}

const fn transform_noise(transform: &[u32; 17], state: u32) -> u32 {
    let mut result = 0;
    let mut bit = 0;
    while bit < 17 {
        if state & (1 << bit) != 0 {
            result ^= transform[bit];
        }
        bit += 1;
    }
    result
}

const fn noise_transforms() -> [[u32; 17]; 64] {
    let mut transforms = [[0; 17]; 64];
    let mut bit = 0;
    while bit < 17 {
        transforms[0][bit] = noise_step(1 << bit);
        bit += 1;
    }
    let mut power = 1;
    while power < 64 {
        bit = 0;
        while bit < 17 {
            transforms[power][bit] =
                transform_noise(&transforms[power - 1], transforms[power - 1][bit]);
            bit += 1;
        }
        power += 1;
    }
    transforms
}

const NOISE_TRANSFORMS: [[u32; 17]; 64] = noise_transforms();

fn advance_noise(mut state: u32, mut steps: u64) -> u32 {
    if steps <= 64 {
        for _ in 0..steps {
            state = noise_step(state);
        }
        return state;
    }
    // Jump the 17-bit LFSR in O(log steps) for a suspended/silent renderer.
    let mut power = 0;
    while steps != 0 {
        if steps & 1 != 0 {
            state = transform_noise(&NOISE_TRANSFORMS[power], state);
        }
        steps >>= 1;
        power += 1;
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(state: u8) -> Note {
        Note {
            state,
            period: 125,
            duration: 100,
            volume: 15,
            volume_env: 0,
            tone_env: 0,
            noise: 0,
        }
    }

    fn active(synth: &CpcSynth, channel: usize) -> ActiveNote {
        synth.channels[channel].active.unwrap()
    }

    fn render_levels(synth: &mut CpcSynth) -> ([f32; 2], [f64; 2]) {
        let mut peaks = [0.0_f32; 2];
        let mut energy = [0.0; 2];
        for _ in 0..4_800 {
            let frame = synth.render_frame(1.0 / 48_000.0);
            for side in 0..2 {
                peaks[side] = peaks[side].max(frame[side].abs());
                energy[side] += f64::from(frame[side]).powi(2);
            }
        }
        (peaks, energy)
    }

    #[test]
    fn centered_channel_preserves_equal_volume_stereo_energy() {
        let outputs: Vec<_> = [1, 2, 4]
            .into_iter()
            .map(|state| {
                let mut synth = CpcSynth::default();
                assert!(synth.enqueue(Note {
                    volume: 12,
                    ..note(state)
                }));
                render_levels(&mut synth)
            })
            .collect();
        assert_eq!(outputs[0].0[1], 0.0);
        assert_eq!(outputs[2].0[0], 0.0);
        assert_eq!(outputs[1].0[0], outputs[1].0[1]);
        assert!((outputs[0].0[0] - 0.213_333_34).abs() < 1e-6);
        let left_energy: f64 = outputs[0].1.iter().sum();
        for (_, energy) in &outputs[1..] {
            assert!((energy.iter().sum::<f64>() / left_energy - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn manual_duet_envelopes_match_cpcbasic_volume_balance() {
        let mut peaks = Vec::new();
        for (state, envelope, attack, decay) in [(1, 1, 5, 8), (2, 2, 7, 12)] {
            let mut synth = CpcSynth::default();
            synth.define_volume(
                envelope,
                vec![
                    Section::Step {
                        steps: 2,
                        delta: attack,
                        ticks: 2,
                    },
                    Section::Step {
                        steps: decay,
                        delta: -1,
                        ticks: 10,
                    },
                    Section::Step {
                        steps: 10,
                        delta: 0,
                        ticks: 15,
                    },
                ],
            );
            assert!(synth.enqueue(Note {
                volume: 0,
                volume_env: envelope,
                ..note(state)
            }));
            peaks.push(render_levels(&mut synth).0);
        }
        // Reference peaks from CPCBasic's applyVolEnv, with our /3 headroom
        // and Web Audio's center-to-stereo downmix. Exercise actual PCM.
        assert!((peaks[0][0] - 0.148_148_15).abs() < 1e-6);
        assert_eq!(peaks[0][1], 0.0);
        assert!((peaks[1][0] - 0.205_322_86).abs() < 1e-6);
        assert_eq!(peaks[1][0], peaks[1][1]);
        assert!((peaks[1][0] / peaks[0][0] - 1.385_929_3).abs() < 1e-6);
        // A mono listener hears the sum of the two speakers. B must not
        // regain the old, roughly eightfold, peak advantage in that mix.
        let mono_melody = (peaks[0][0] + peaks[0][1]) * 0.5;
        let mono_bass = (peaks[1][0] + peaks[1][1]) * 0.5;
        assert!((mono_bass / mono_melody - 2.771_858_6).abs() < 1e-6);
    }

    #[test]
    fn classic_volume_zero_is_silent_and_full_mix_has_headroom() {
        for noise in [0, 31] {
            for volume in [0, 15] {
                let mut synth = CpcSynth::default();
                assert!(synth.enqueue(Note {
                    volume,
                    noise,
                    ..note(7)
                }));
                let (peaks, _) = render_levels(&mut synth);
                if volume == 0 {
                    assert_eq!(peaks, [0.0, 0.0]);
                } else {
                    assert!(peaks.iter().all(|peak| *peak > 0.0 && *peak < 1.0));
                }
            }
        }
    }

    #[test]
    fn period_sets_pitch_and_stereo_channel() {
        let mut synth = CpcSynth::default();
        assert!(synth.enqueue(note(1)));
        let mut rising = 0;
        let mut previous = 0.0;
        for _ in 0..48_000 {
            let frame = synth.render_frame(1.0 / 48_000.0);
            assert_eq!(frame[1], 0.0);
            if frame[0] > 0.0 && previous <= 0.0 {
                rising += 1;
            }
            previous = frame[0];
        }
        assert_eq!(rising, 500); // 62500 / 125, independent expected frequency.
        assert!(synth.is_idle());
    }

    #[test]
    fn held_note_status_and_release_match_manual() {
        let mut synth = CpcSynth::default();
        assert!(synth.enqueue(note(65)));
        assert_eq!(synth.status(1), 67);
        synth.advance(100.0);
        assert_eq!(synth.status(1), 67);
        synth.release(1);
        assert_eq!(synth.status(1), 132);
        synth.advance(0.99);
        assert_eq!(synth.status(1), 132);
        synth.advance(0.01);
        assert_eq!(synth.status(1), 4);
    }

    #[test]
    fn releasing_current_hold_does_not_release_future_notes() {
        let mut synth = CpcSynth::default();
        synth.enqueue(note(65));
        synth.enqueue(note(65));
        synth.release(1);
        assert_eq!(synth.status(1), 131);
        synth.release(1); // Already playing: the future held note stays held.
        synth.advance(1.0);
        assert_eq!(synth.status(1), 67);
    }

    #[test]
    fn full_multi_channel_enqueue_is_atomic_and_flush_replaces_all() {
        let mut synth = CpcSynth::default();
        for _ in 0..4 {
            assert!(synth.enqueue(note(65)));
        }
        assert!(!synth.enqueue(note(3)));
        assert_eq!(synth.status(1), 64);
        assert_eq!(synth.status(2), 4);
        assert!(synth.enqueue(note(131)));
        assert_eq!(synth.status(1), 132);
        assert_eq!(synth.status(2), 132);
        synth.advance(1.0);
        assert!(synth.is_idle());
    }

    #[test]
    fn four_pending_slots_are_separate_from_the_playing_note() {
        let mut synth = CpcSynth::default();
        for _ in 0..5 {
            assert!(synth.enqueue(note(1)));
        }
        assert_eq!(synth.status(1), 128);
        assert!(!synth.enqueue(note(1)));
        synth.advance(1.0);
        assert_eq!(synth.status(1), 129);
        synth.advance(4.0);
        assert!(synth.is_idle());
    }

    #[test]
    fn rendezvous_requires_reciprocal_heads_and_starts_together() {
        let mut synth = CpcSynth::default();
        synth.enqueue(note(17)); // A awaits B.
        assert_eq!(synth.status(1), 19);
        synth.enqueue(note(2)); // B without rendezvous may play independently.
        assert_eq!(synth.status(1), 19);
        synth.enqueue(note(10)); // B awaits A, behind the playing note.
        synth.advance(1.0);
        assert_eq!(synth.status(1), 132);
        assert_eq!(synth.status(2), 132);
        assert_eq!(active(&synth, 0).remaining, active(&synth, 1).remaining);
        assert_eq!(active(&synth, 0).phase, active(&synth, 1).phase);
    }

    #[test]
    fn multi_channel_notes_implicitly_rendezvous() {
        let mut synth = CpcSynth::default();
        synth.enqueue(note(1));
        synth.enqueue(note(3));
        assert_eq!(synth.status(2), 11);
        synth.advance(1.0);
        assert_eq!(synth.status(1), 132);
        assert_eq!(synth.status(2), 132);
        assert_eq!(active(&synth, 0).phase, active(&synth, 1).phase);
    }

    #[test]
    fn volume_first_step_is_immediate_wraps_and_duration_zero_uses_length() {
        let mut synth = CpcSynth::default();
        synth.define_volume(
            1,
            vec![Section::Step {
                steps: 3,
                delta: 1,
                ticks: 10,
            }],
        );
        synth.enqueue(Note {
            duration: 0,
            volume: 14,
            volume_env: 1,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).volume, 15);
        synth.advance(0.1);
        assert_eq!(active(&synth, 0).volume, 0);
        synth.advance(0.1);
        assert_eq!(active(&synth, 0).volume, 1);
        synth.advance(0.1);
        assert!(synth.is_idle());
    }

    #[test]
    fn negative_duration_repeats_volume_envelope_from_initial_volume() {
        let mut synth = CpcSynth::default();
        synth.define_volume(
            1,
            vec![Section::Step {
                steps: 2,
                delta: -1,
                ticks: 5,
            }],
        );
        synth.enqueue(Note {
            duration: -3,
            volume_env: 1,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).volume, 14);
        synth.advance(0.05);
        assert_eq!(active(&synth, 0).volume, 13);
        synth.advance(0.05);
        assert_eq!(active(&synth, 0).volume, 14);
        synth.advance(0.2);
        assert!(synth.is_idle());
    }

    #[test]
    fn zero_steps_absolute_volume_and_tone_pause() {
        let mut synth = CpcSynth::default();
        synth.define_volume(
            1,
            vec![Section::Step {
                steps: 0,
                delta: -1,
                ticks: 10,
            }],
        );
        synth.define_tone(
            1,
            false,
            vec![
                Section::Step {
                    steps: 0,
                    delta: 99,
                    ticks: 10,
                },
                Section::Absolute {
                    value: 250,
                    ticks: 10,
                },
            ],
        );
        synth.enqueue(Note {
            volume: 0,
            volume_env: 1,
            tone_env: 1,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).volume, 15);
        assert_eq!(active(&synth, 0).period, 125);
        synth.advance(0.1);
        assert_eq!(active(&synth, 0).period, 250);
    }

    #[test]
    fn repeating_tone_envelope_accumulates_period_changes() {
        let mut synth = CpcSynth::default();
        synth.define_tone(
            1,
            true,
            vec![Section::Step {
                steps: 2,
                delta: 10,
                ticks: 5,
            }],
        );
        synth.enqueue(Note {
            tone_env: 1,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).period, 135);
        synth.advance(0.05);
        assert_eq!(active(&synth, 0).period, 145);
        synth.advance(0.05);
        assert_eq!(active(&synth, 0).period, 155);
        synth.advance(0.05);
        assert_eq!(active(&synth, 0).period, 165);
    }

    #[test]
    fn completed_envelopes_hold_last_values_and_short_notes_end_early() {
        let mut synth = CpcSynth::default();
        synth.define_volume(
            1,
            vec![Section::Step {
                steps: 1,
                delta: -1,
                ticks: 1,
            }],
        );
        synth.define_tone(
            1,
            false,
            vec![Section::Step {
                steps: 2,
                delta: 10,
                ticks: 1,
            }],
        );
        synth.enqueue(Note {
            volume_env: 1,
            tone_env: 1,
            ..note(1)
        });
        synth.advance(0.8);
        assert_eq!(active(&synth, 0).volume, 14);
        assert_eq!(active(&synth, 0).period, 145);
        synth.stop();
        synth.enqueue(Note {
            duration: 1,
            volume_env: 1,
            tone_env: 1,
            ..note(1)
        });
        synth.advance(0.01);
        assert!(synth.is_idle());
    }

    #[test]
    fn undefined_volume_envelope_defaults_to_two_seconds() {
        let mut synth = CpcSynth::default();
        synth.enqueue(Note {
            duration: 0,
            volume_env: 9,
            ..note(1)
        });
        synth.advance(1.99);
        assert!(!synth.is_idle());
        synth.advance(0.01);
        assert!(synth.is_idle());
        synth.enqueue(Note {
            duration: -2,
            ..note(1)
        });
        synth.advance(4.0);
        assert!(synth.is_idle());
    }

    #[test]
    fn noise_is_shared_and_zero_period_tone_is_silent() {
        let mut synth = CpcSynth::default();
        synth.enqueue(Note {
            period: 0,
            ..note(1)
        });
        assert_eq!(synth.render_frame(1.0 / 48_000.0), [0.0, 0.0]);
        synth.stop();
        synth.enqueue(Note {
            period: 0,
            noise: 3,
            ..note(1)
        });
        synth.enqueue(Note {
            period: 0,
            noise: 7,
            ..note(4)
        });
        assert_eq!(synth.noise_period, 7);
        let mut different = false;
        let mut previous = [0.0, 0.0];
        for _ in 0..1000 {
            let frame = synth.render_frame(1.0 / 48_000.0);
            assert_eq!(frame[0], frame[1]);
            different |= frame != previous;
            previous = frame;
        }
        assert!(different);
        synth.stop();
        assert_eq!(synth.render_frame(1.0 / 48_000.0), [0.0, 0.0]);
    }

    #[test]
    fn silent_advance_matches_frame_rendering_and_noise_skip() {
        let mut rendered = CpcSynth::default();
        let mut skipped = CpcSynth::default();
        for synth in [&mut rendered, &mut skipped] {
            synth.define_tone(
                1,
                true,
                vec![Section::Step {
                    steps: 3,
                    delta: 7,
                    ticks: 2,
                }],
            );
            synth.enqueue(Note {
                tone_env: 1,
                noise: 2,
                ..note(1)
            });
        }
        for _ in 0..24_000 {
            rendered.render_frame(1.0 / 48_000.0);
        }
        skipped.advance(0.5);
        assert_eq!(active(&rendered, 0).period, active(&skipped, 0).period);
        assert!((active(&rendered, 0).remaining - active(&skipped, 0).remaining).abs() < 1.0e-9);
        assert!((active(&rendered, 0).phase - active(&skipped, 0).phase).abs() < 1.0e-8);
        assert_eq!(rendered.noise_state, skipped.noise_state);
        for count in [0, 1, 64, 65, 1000, 100_000] {
            let mut sequential = 0x1ffff;
            for _ in 0..count {
                sequential = noise_step(sequential);
            }
            assert_eq!(advance_noise(0x1ffff, count), sequential);
        }
    }

    #[test]
    fn stop_preserves_definitions_but_clear_discards_them() {
        let mut synth = CpcSynth::default();
        synth.define_volume(
            1,
            vec![Section::Absolute {
                value: 3,
                ticks: 256,
            }],
        );
        synth.stop();
        synth.enqueue(Note {
            volume_env: 1,
            duration: 0,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).volume, 3);
        assert!((active(&synth, 0).remaining - 2.56).abs() < EPSILON);
        synth.clear();
        assert!(synth.is_idle());
        synth.enqueue(Note {
            volume_env: 1,
            duration: 0,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).volume, 15);
        assert_eq!(active(&synth, 0).remaining, 2.0);
    }

    #[test]
    fn all_three_channels_wait_for_held_rendezvous_partner() {
        let mut synth = CpcSynth::default();
        synth.enqueue(note(1 | 16 | 32));
        synth.enqueue(note(2 | 8 | 32 | 64));
        synth.enqueue(note(4 | 8 | 16));
        assert_eq!(synth.status(1), 3 | 16 | 32);
        assert_eq!(synth.status(2), 67);
        assert_eq!(synth.status(4), 3 | 8 | 16);
        synth.release(2);
        for channel in [1, 2, 4] {
            assert_eq!(synth.status(channel), 132);
        }
        synth.advance(1.0);
        assert!(synth.is_idle());
    }

    #[test]
    fn pending_notes_resolve_envelopes_when_they_start() {
        let mut synth = CpcSynth::default();
        synth.define_volume(
            1,
            vec![Section::Absolute {
                value: 3,
                ticks: 10,
            }],
        );
        synth.enqueue(Note {
            volume_env: 1,
            ..note(1)
        });
        synth.enqueue(Note {
            volume_env: 1,
            ..note(1)
        });
        synth.define_volume(
            1,
            vec![Section::Absolute {
                value: 7,
                ticks: 10,
            }],
        );
        assert_eq!(active(&synth, 0).volume, 3);
        synth.advance(1.0);
        assert_eq!(active(&synth, 0).volume, 7);
    }

    #[test]
    fn tone_period_limits_and_alias_suppression_are_explicit() {
        let mut synth = CpcSynth::default();
        synth.define_tone(
            1,
            true,
            vec![Section::Step {
                steps: 1,
                delta: -128,
                ticks: 1,
            }],
        );
        synth.enqueue(Note {
            tone_env: 1,
            period: 100,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).period, 0);
        synth.advance(0.1);
        assert_eq!(active(&synth, 0).period, 0);
        synth.stop();
        synth.define_tone(
            1,
            true,
            vec![Section::Step {
                steps: 1,
                delta: 127,
                ticks: 1,
            }],
        );
        synth.enqueue(Note {
            tone_env: 1,
            period: 4090,
            ..note(1)
        });
        assert_eq!(active(&synth, 0).period, 4095);
        assert_eq!(square_wave(0.25, 0.75), 0.0);
    }

    #[test]
    fn several_minutes_without_a_device_advance_envelopes_and_finish() {
        let mut synth = CpcSynth::default();
        synth.define_tone(
            1,
            true,
            vec![Section::Step {
                steps: 1,
                delta: 1,
                ticks: 1,
            }],
        );
        synth.enqueue(Note {
            duration: 30_000,
            tone_env: 1,
            noise: 1,
            ..note(7)
        });
        synth.advance(299.99);
        assert_eq!(synth.status(1), 132);
        assert_eq!(active(&synth, 0).period, 4095);
        synth.advance(0.01);
        assert!(synth.is_idle());
    }
}
