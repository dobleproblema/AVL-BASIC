//! Shared CPC state with nonblocking status reads and an output-specific clock.

use super::cpc::{CpcSynth, Note, Section};
use kira::sound::{Sound, SoundData};
use kira::{info::Info, Frame};
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

const IDLE: u32 = 1 << 24;
const EMPTY_STATUS: u32 = IDLE | 4 | (4 << 8) | (4 << 16);

#[derive(Debug)]
struct Epoch {
    valid: AtomicBool,
    missed_nanos: AtomicU64,
}

impl Epoch {
    fn new() -> Self {
        Self {
            valid: AtomicBool::new(true),
            missed_nanos: AtomicU64::new(0),
        }
    }

    fn miss(&self, seconds: f64) {
        if !self.valid.load(Ordering::Acquire) || !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let nanos = (seconds * 1_000_000_000.0).round() as u64;
        let _ = self
            .missed_nanos
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some(old.saturating_add(nanos))
            });
    }

    fn take_seconds(&self) -> f64 {
        self.missed_nanos.swap(0, Ordering::AcqRel) as f64 / 1_000_000_000.0
    }
}

#[derive(Debug, Default)]
struct State {
    synth: CpcSynth,
    epoch: Option<Arc<Epoch>>,
}

impl State {
    fn drain_debt(&mut self) {
        if let Some(epoch) = &self.epoch {
            self.synth.advance(epoch.take_seconds());
        }
    }

    fn retire_output(&mut self) {
        if let Some(epoch) = self.epoch.take() {
            // The mutex orders retirement against successful render callbacks.
            // A late failed callback can only add debt to this discarded epoch.
            epoch.valid.store(false, Ordering::Release);
            self.synth.advance(epoch.take_seconds());
        }
    }

    fn discard_debt(&self) {
        if let Some(epoch) = &self.epoch {
            epoch.missed_nanos.store(0, Ordering::Release);
        }
    }
}

#[derive(Debug)]
pub(super) struct SharedCpc {
    state: Mutex<State>,
    snapshot: AtomicU32,
}

impl Default for SharedCpc {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            snapshot: AtomicU32::new(EMPTY_STATUS),
        }
    }
}

impl SharedCpc {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn publish(&self, synth: &CpcSynth) {
        let snapshot = u32::from(synth.status(1))
            | (u32::from(synth.status(2)) << 8)
            | (u32::from(synth.status(4)) << 16)
            | if synth.is_idle() { IDLE } else { 0 };
        self.snapshot.store(snapshot, Ordering::Release);
    }

    fn control<T>(&self, action: impl FnOnce(&mut CpcSynth) -> T) -> T {
        let mut state = self.lock();
        // Apply elapsed output time before changing the queue: newly submitted
        // notes must not inherit the debt of notes that preceded them.
        state.drain_debt();
        let result = action(&mut state.synth);
        self.publish(&state.synth);
        result
    }

    pub(super) fn begin_output(self: &Arc<Self>) -> CpcSound {
        let mut state = self.lock();
        state.retire_output();
        let epoch = Arc::new(Epoch::new());
        state.epoch = Some(epoch.clone());
        self.publish(&state.synth);
        CpcSound {
            shared: self.clone(),
            epoch,
        }
    }

    pub(super) fn end_output(&self) {
        let mut state = self.lock();
        state.retire_output();
        self.publish(&state.synth);
    }

    pub(super) fn enqueue(&self, note: Note) -> bool {
        self.control(|synth| synth.enqueue(note))
    }

    pub(super) fn define_volume(&self, id: u8, sections: Vec<Section>) {
        self.control(|synth| synth.define_volume(id, sections));
    }

    pub(super) fn define_tone(&self, id: u8, repeating: bool, sections: Vec<Section>) {
        self.control(|synth| synth.define_tone(id, repeating, sections));
    }

    pub(super) fn release(&self, mask: u8) {
        self.control(|synth| synth.release(mask));
    }

    pub(super) fn status(&self, channel: u8) -> u8 {
        let shift = match channel {
            1 => 0,
            2 => 8,
            4 => 16,
            _ => return 0,
        };
        (self.snapshot.load(Ordering::Acquire) >> shift) as u8
    }

    pub(super) fn is_idle(&self) -> bool {
        self.snapshot.load(Ordering::Acquire) & IDLE != 0
    }

    pub(super) fn advance(&self, seconds: f64) {
        self.control(|synth| synth.advance(seconds));
    }

    pub(super) fn stop(&self) {
        let mut state = self.lock();
        state.drain_debt();
        state.synth.stop();
        state.discard_debt();
        self.publish(&state.synth);
    }

    pub(super) fn clear(&self) {
        let mut state = self.lock();
        state.drain_debt();
        state.synth.clear();
        state.discard_debt();
        self.publish(&state.synth);
    }
}

pub(super) struct CpcSound {
    shared: Arc<SharedCpc>,
    epoch: Arc<Epoch>,
}

impl CpcSound {
    fn render_block(&mut self, out: &mut [Frame], dt: f64) {
        out.fill(Frame::ZERO);
        if !self.epoch.valid.load(Ordering::Acquire) {
            return;
        }
        let mut state = match self.shared.state.try_lock() {
            Ok(state) => state,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => {
                self.epoch.miss(dt * out.len() as f64);
                return;
            }
        };
        // An output may have been retired between the first check and locking.
        // It must never advance notes submitted to a later output instance.
        if !self.epoch.valid.load(Ordering::Acquire)
            || !state
                .epoch
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &self.epoch))
        {
            return;
        }
        state.drain_debt();
        for frame in out {
            let [left, right] = state.synth.render_frame(dt);
            *frame = Frame::new(left, right);
        }
        self.shared.publish(&state.synth);
    }
}

impl SoundData for CpcSound {
    type Error = Infallible;
    type Handle = ();

    fn into_sound(self) -> Result<(Box<dyn Sound>, ()), Infallible> {
        Ok((Box::new(self), ()))
    }
}

impl Sound for CpcSound {
    fn process(&mut self, out: &mut [Frame], dt: f64, _: &Info) {
        self.render_block(out, dt);
    }

    fn finished(&self) -> bool {
        !self.epoch.valid.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(duration: i16) -> Note {
        Note {
            state: 1,
            period: 142,
            duration,
            volume: 8,
            volume_env: 0,
            tone_env: 0,
            noise: 0,
        }
    }

    fn miss_block(shared: &SharedCpc, sound: &mut CpcSound, seconds: f64) {
        let _held = shared.lock();
        let mut out = [Frame::new(1.0, 1.0)];
        sound.render_block(&mut out, seconds);
        assert_eq!(out[0], Frame::ZERO);
    }

    #[test]
    fn status_and_idle_reads_do_not_acquire_the_renderer_mutex() {
        let shared = SharedCpc::default();
        assert!(shared.enqueue(Note {
            state: 65,
            ..note(100)
        }));
        let _held = shared.lock();
        assert_eq!(shared.status(1), 67);
        assert_eq!(shared.status(2), 4);
        assert_eq!(shared.status(4), 4);
        assert!(!shared.is_idle());
    }

    #[test]
    fn rendering_recovers_time_from_a_block_lost_to_contention() {
        let shared = Arc::new(SharedCpc::default());
        let mut sound = shared.begin_output();
        assert!(shared.enqueue(note(100)));
        miss_block(&shared, &mut sound, 0.5);
        assert_eq!(shared.status(1), 132);
        sound.render_block(&mut [Frame::ZERO], 0.5);
        assert_eq!(shared.status(1), 4);
        assert!(shared.is_idle());
    }

    #[test]
    fn controls_drain_output_debt_before_submitting_new_notes() {
        let shared = Arc::new(SharedCpc::default());
        let mut sound = shared.begin_output();
        assert!(shared.enqueue(note(50)));
        miss_block(&shared, &mut sound, 0.5);
        assert!(shared.enqueue(note(100)));
        // The first note ended before enqueue, leaving all pending slots free.
        assert_eq!(shared.status(1), 132);
        sound.render_block(&mut [Frame::ZERO], 0.99);
        assert!(!shared.is_idle());
        sound.render_block(&mut [Frame::ZERO], 0.01);
        assert!(shared.is_idle());
    }

    #[test]
    fn retired_callback_cannot_render_or_advance_a_new_outputs_notes() {
        let shared = Arc::new(SharedCpc::default());
        let mut old = shared.begin_output();
        assert!(shared.enqueue(note(50)));
        miss_block(&shared, &mut old, 0.5);
        shared.end_output();
        assert!(shared.is_idle());
        assert!(old.finished());
        let mut current = shared.begin_output();
        assert!(shared.enqueue(note(100)));
        let mut out = [Frame::new(1.0, 1.0)];
        old.render_block(&mut out, 10.0);
        assert_eq!(out[0], Frame::ZERO);
        assert_eq!(shared.status(1), 132);
        assert_eq!(old.epoch.missed_nanos.load(Ordering::Acquire), 0);
        current.render_block(&mut out, 0.99);
        assert!(!shared.is_idle());
        current.render_block(&mut out, 0.01);
        assert!(shared.is_idle());
    }

    #[test]
    fn replacing_output_retires_the_previous_epoch() {
        let shared = Arc::new(SharedCpc::default());
        let mut old = shared.begin_output();
        let _current = shared.begin_output();
        assert!(shared.enqueue(note(100)));
        old.render_block(&mut [Frame::ZERO], 2.0);
        assert!(old.finished());
        assert_eq!(shared.status(1), 132);
    }

    #[test]
    fn stop_and_clear_discard_debt_before_new_notes_are_submitted() {
        for clear in [false, true] {
            let shared = Arc::new(SharedCpc::default());
            let mut sound = shared.begin_output();
            assert!(shared.enqueue(note(100)));
            miss_block(&shared, &mut sound, 0.9);
            if clear {
                shared.clear();
            } else {
                shared.stop();
            }
            assert!(shared.is_idle());
            assert_eq!(sound.epoch.missed_nanos.load(Ordering::Acquire), 0);
            assert!(shared.enqueue(note(100)));
            sound.render_block(&mut [Frame::ZERO], 0.99);
            assert!(!shared.is_idle());
            sound.render_block(&mut [Frame::ZERO], 0.01);
            assert!(shared.is_idle());
        }
    }
}
