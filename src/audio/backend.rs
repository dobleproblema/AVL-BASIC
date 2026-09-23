//! Audio output adapters for Kira: CPAL and a bounded WSLg Pulse connection.
//!
//! Device errors are returned to the interpreter. In particular, a failed device
//! change never triggers a panic or an implicit restart of a BASIC program's audio.

#[cfg(target_os = "linux")]
mod pulse;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kira::backend::{Backend, Renderer};
use kira::{AudioManager, AudioManagerSettings};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(target_os = "linux")]
const PULSE_CALLBACK_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn open_output() -> Result<AudioManager<OutputBackend>, String> {
    fn create() -> Result<AudioManager<OutputBackend>, String> {
        AudioManager::new(AudioManagerSettings {
            backend_settings: BackendSettings::default(),
            ..Default::default()
        })
    }

    #[cfg(target_os = "linux")]
    {
        static OPENING: AtomicBool = AtomicBool::new(false);
        // CPAL bounds stream creation, but Pulse device lookup and uncork also
        // await server replies. Bound the complete opening operation here.
        open_with_timeout(&OPENING, Duration::from_secs(3), create)
    }
    #[cfg(not(target_os = "linux"))]
    create()
}

#[cfg(any(target_os = "linux", test))]
fn open_with_timeout<T, F>(
    opening: &'static AtomicBool,
    timeout: Duration,
    create: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    if opening
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("Previous audio initialization is still pending".into());
    }
    struct OpeningGuard(&'static AtomicBool);
    impl Drop for OpeningGuard {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let guard = OpeningGuard(opening);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("avl-audio-open".into())
        .spawn(move || {
            let _guard = guard;
            // If BASIC already timed out, the returned manager is dropped here.
            // No program sounds are attached until BASIC receives the manager.
            let _ = sender.send(create());
        })
        .map_err(|error| format!("Cannot start audio initialization worker: {error}"))?;
    receiver
        .recv_timeout(timeout)
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => {
                "Audio initialization timed out".to_string()
            }
            std::sync::mpsc::RecvTimeoutError::Disconnected => {
                "Audio initialization worker stopped unexpectedly".to_string()
            }
        })?
}

struct CallbackWatchdog {
    timeout: Duration,
    frames: u64,
    progressed_at: Instant,
    window_frames: u64,
    window_started: Instant,
}

impl CallbackWatchdog {
    fn new(frames: u64, now: Instant) -> Self {
        Self::with_timeout(frames, now, CALLBACK_TIMEOUT)
    }

    fn with_timeout(frames: u64, now: Instant, timeout: Duration) -> Self {
        Self {
            timeout,
            frames,
            progressed_at: now,
            window_frames: frames,
            window_started: now,
        }
    }

    fn stalled(&mut self, frames: u64, now: Instant, sample_rate: u32) -> bool {
        if frames != self.frames {
            self.frames = frames;
            self.progressed_at = now;
        }
        if now.saturating_duration_since(self.progressed_at) >= self.timeout {
            return true;
        }
        let elapsed = now.saturating_duration_since(self.window_started);
        if elapsed >= self.timeout {
            // Occasional callbacks are not sufficient: a degraded output can
            // trickle samples for minutes while BASIC waits for a short note.
            // Allow generous scheduling jitter, but reject sustained playback
            // below half the requested sample rate over the configured window.
            let played = frames.wrapping_sub(self.window_frames) as f64 / sample_rate as f64;
            self.window_frames = frames;
            self.window_started = now;
            return played < elapsed.as_secs_f64() * 0.5;
        }
        false
    }
}

#[cfg(any(target_os = "linux", test))]
fn needs_wsl_pulse(server: Option<&str>, distro: bool, interop: bool) -> bool {
    distro || interop || server.is_some_and(|value| value.contains("/mnt/wslg/"))
}

// Pulse's Stream::drop joins workers that may be waiting for an unresponsive
// server. Keep that wait off the BASIC thread. A single reservation covers both
// the live stream and its retirement, so a stuck cleanup prevents new streams
// instead of accumulating threads or abandoned streams on every RUN.
#[cfg(target_os = "linux")]
mod pulse_cleanup {
    use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};
    use std::time::Duration;

    type Gate = Arc<(Mutex<bool>, Condvar)>;

    struct Reaper {
        sender: mpsc::SyncSender<cpal::Stream>,
        gate: Gate,
    }

    fn release(gate: &Gate) {
        let mut available = gate.0.lock().unwrap_or_else(|error| error.into_inner());
        *available = true;
        gate.1.notify_one();
    }

    impl Reaper {
        fn new() -> Result<Self, String> {
            let (sender, receiver) = mpsc::sync_channel::<cpal::Stream>(1);
            let gate = Arc::new((Mutex::new(true), Condvar::new()));
            let worker_gate = gate.clone();
            std::thread::Builder::new()
                .name("avl-audio-cleanup".into())
                .spawn(move || {
                    while let Ok(stream) = receiver.recv() {
                        drop(stream);
                        release(&worker_gate);
                    }
                })
                .map_err(|error| format!("Cannot start audio cleanup worker: {error}"))?;
            Ok(Self { sender, gate })
        }
    }

    pub(super) struct Reservation(Option<&'static Reaper>);

    impl Reservation {
        pub(super) fn acquire() -> Result<Self, String> {
            static REAPER: OnceLock<Result<Reaper, String>> = OnceLock::new();
            let reaper = REAPER
                .get_or_init(Reaper::new)
                .as_ref()
                .map_err(|error| error.clone())?;
            let available = reaper
                .gate
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let (mut available, _) = reaper
                .gate
                .1
                .wait_timeout_while(available, Duration::from_millis(250), |value| !*value)
                .unwrap_or_else(|error| error.into_inner());
            if !*available {
                return Err(
                    "Previous PulseAudio output is still closing; audio output is unavailable"
                        .into(),
                );
            }
            *available = false;
            Ok(Self(Some(reaper)))
        }

        pub(super) fn retire(mut self, stream: cpal::Stream) {
            if let Some(reaper) = self.0.take() {
                if let Err(error) = reaper.sender.try_send(stream) {
                    // The reservation makes a full queue impossible. If the worker
                    // is lost, quarantine this one stream and leave the gate closed;
                    // dropping it here could hang BASIC, and reopening could leak more.
                    match error {
                        mpsc::TrySendError::Full(stream)
                        | mpsc::TrySendError::Disconnected(stream) => std::mem::forget(stream),
                    }
                }
            }
        }
    }

    impl Drop for Reservation {
        fn drop(&mut self) {
            if let Some(reaper) = self.0.take() {
                release(&reaper.gate);
            }
        }
    }
}

#[derive(Default)]
pub(super) struct BackendSettings {
    pub disabled: bool,
}

#[derive(Default)]
struct Failure {
    failed: AtomicBool,
    frames: AtomicU64,
    message: Mutex<Option<String>>,
}

impl Failure {
    fn record(&self, message: String) {
        if let Ok(mut slot) = self.message.try_lock() {
            if slot.is_none() {
                *slot = Some(message);
            }
        }
        self.failed.store(true, Ordering::Release);
    }

    fn message(&self) -> Option<String> {
        if !self.failed.load(Ordering::Acquire) {
            return None;
        }
        Some(
            self.message
                .lock()
                .ok()
                .and_then(|value| value.clone())
                .unwrap_or_else(|| "Audio output failed".to_string()),
        )
    }
}

pub(super) struct OutputBackend {
    device: Option<cpal::Device>,
    config: cpal::StreamConfig,
    format: cpal::SampleFormat,
    stream: Option<cpal::Stream>,
    failure: Arc<Failure>,
    watchdog: CallbackWatchdog,
    #[cfg(target_os = "linux")]
    pulse_cleanup: Option<pulse_cleanup::Reservation>,
    #[cfg(target_os = "linux")]
    pulse: Option<pulse::PulseOutput>,
}

impl OutputBackend {
    pub fn error(&mut self) -> Option<String> {
        let started = self.stream.is_some();
        #[cfg(target_os = "linux")]
        let started = started
            || self
                .pulse
                .as_ref()
                .is_some_and(|output| output.is_started());
        if started
            && self.watchdog.stalled(
                self.failure.frames.load(Ordering::Relaxed),
                Instant::now(),
                self.config.sample_rate,
            )
        {
            self.failure
                .record("Audio output stopped responding or playback became too slow".into());
        }
        self.failure.message()
    }

    fn build_stream<T>(
        &self,
        config: &cpal::StreamConfig,
        renderer: Arc<Mutex<Renderer>>,
    ) -> Result<cpal::Stream, String>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels as usize;
        // CPAL may supply larger buffers than requested. Process them in bounded,
        // frame-aligned chunks without allocating on the audio thread.
        let mut scratch = vec![0.0_f32; channels * 1024];
        let failure = self.failure.clone();
        let callback_failure = failure.clone();
        self.device
            .as_ref()
            .ok_or_else(|| "No CPAL device was selected".to_string())?
            .build_output_stream(
                config.clone(),
                move |out: &mut [T], _| {
                    out.fill(T::from_sample(0.0));
                    if failure.failed.load(Ordering::Acquire) {
                        return;
                    }
                    let Ok(mut renderer) = renderer.try_lock() else {
                        return;
                    };
                    renderer.on_start_processing();
                    for chunk in out.chunks_mut(scratch.len()) {
                        let aligned = chunk.len() / channels * channels;
                        if aligned == 0 {
                            continue;
                        }
                        renderer.process(&mut scratch[..aligned], channels as u16);
                        for (target, sample) in chunk.iter_mut().zip(&scratch[..aligned]) {
                            *target = T::from_sample(*sample);
                        }
                    }
                    failure
                        .frames
                        .fetch_add((out.len() / channels) as u64, Ordering::Relaxed);
                },
                move |error| callback_failure.record(error.to_string()),
                Some(Duration::from_secs(2)),
            )
            .map_err(|error| error.to_string())
    }

    fn build(
        &self,
        config: &cpal::StreamConfig,
        renderer: Arc<Mutex<Renderer>>,
    ) -> Result<cpal::Stream, String> {
        use cpal::SampleFormat;
        match self.format {
            SampleFormat::F32 => self.build_stream::<f32>(config, renderer),
            SampleFormat::F64 => self.build_stream::<f64>(config, renderer),
            SampleFormat::I8 => self.build_stream::<i8>(config, renderer),
            SampleFormat::I16 => self.build_stream::<i16>(config, renderer),
            SampleFormat::I24 => self.build_stream::<cpal::I24>(config, renderer),
            SampleFormat::I32 => self.build_stream::<i32>(config, renderer),
            SampleFormat::I64 => self.build_stream::<i64>(config, renderer),
            SampleFormat::U8 => self.build_stream::<u8>(config, renderer),
            SampleFormat::U16 => self.build_stream::<u16>(config, renderer),
            SampleFormat::U24 => self.build_stream::<cpal::U24>(config, renderer),
            SampleFormat::U32 => self.build_stream::<u32>(config, renderer),
            SampleFormat::U64 => self.build_stream::<u64>(config, renderer),
            _ => Err("Unsupported audio output sample format".to_string()),
        }
    }
}

impl Drop for OutputBackend {
    fn drop(&mut self) {
        // A retired stream that recovers must produce silence, never consume the
        // renderer belonging to an earlier program.
        self.failure.failed.store(true, Ordering::Release);
        #[cfg(target_os = "linux")]
        self.pulse.take();
        #[cfg(target_os = "linux")]
        if let Some(cleanup) = self.pulse_cleanup.take() {
            if let Some(stream) = self.stream.take() {
                cleanup.retire(stream);
            }
        }
    }
}

impl Backend for OutputBackend {
    type Settings = BackendSettings;
    type Error = String;

    fn setup(settings: BackendSettings, _: usize) -> Result<(Self, u32), String> {
        if settings.disabled {
            return Err("Audio output is disabled".to_string());
        }
        #[cfg(target_os = "linux")]
        {
            let server = std::env::var("PULSE_SERVER").ok();
            if needs_wsl_pulse(
                server.as_deref(),
                std::env::var_os("WSL_DISTRO_NAME").is_some(),
                std::env::var_os("WSL_INTEROP").is_some(),
            ) {
                // CPAL 0.18.2's fixed Pulse buffer sets tlength = 2 * minreq.
                // Pulse consequently requests zero sink latency, clamped by
                // WSLg to 0.5 ms, causing excessive Pulse-to-Weston wakeups.
                // Use the same Rust protocol dependency with independent
                // buffer bounds and an interruptible connection for WSLg.
                let output = pulse::PulseOutput::connect()
                    .map_err(|error| format!("WSLg PulseAudio output is unavailable: {error}"))?;
                return Ok((
                    Self {
                        device: None,
                        config: cpal::StreamConfig {
                            channels: 2,
                            sample_rate: pulse::SAMPLE_RATE,
                            buffer_size: cpal::BufferSize::Default,
                        },
                        format: cpal::SampleFormat::I16,
                        stream: None,
                        failure: Arc::new(Failure::default()),
                        watchdog: CallbackWatchdog::with_timeout(
                            0,
                            Instant::now(),
                            PULSE_CALLBACK_TIMEOUT,
                        ),
                        pulse_cleanup: None,
                        pulse: Some(output),
                    },
                    pulse::SAMPLE_RATE,
                ));
            }
        }
        let host = cpal::default_host();
        #[cfg(target_os = "linux")]
        let pulse_cleanup = if host.id() == cpal::HostId::PulseAudio {
            Some(pulse_cleanup::Reservation::acquire()?)
        } else {
            None
        };
        let device = host
            .default_output_device()
            .ok_or_else(|| "No audio output device is available".to_string())?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let mut config = supported.config();
        if config.channels == 0 || config.channels > 64 || config.sample_rate == 0 {
            return Err("Unsupported audio output configuration".to_string());
        }
        let requested = match supported.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => 1024_u32.clamp(*min, *max),
            cpal::SupportedBufferSize::Unknown => 1024,
        };
        config.buffer_size = cpal::BufferSize::Fixed(requested.max(1));
        let sample_rate = config.sample_rate;
        Ok((
            Self {
                device: Some(device),
                config,
                format: supported.sample_format(),
                stream: None,
                failure: Arc::new(Failure::default()),
                watchdog: CallbackWatchdog::new(0, Instant::now()),
                #[cfg(target_os = "linux")]
                pulse_cleanup,
                #[cfg(target_os = "linux")]
                pulse: None,
            },
            sample_rate,
        ))
    }

    fn start(&mut self, renderer: Renderer) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        if let Some(output) = &mut self.pulse {
            output.start(renderer, self.failure.clone())?;
            self.watchdog = CallbackWatchdog::with_timeout(
                self.failure.frames.load(Ordering::Relaxed),
                Instant::now(),
                PULSE_CALLBACK_TIMEOUT,
            );
            return Ok(());
        }
        let renderer = Arc::new(Mutex::new(renderer));
        let stream = match self.build(&self.config, renderer.clone()) {
            Ok(stream) => stream,
            Err(first_error) => {
                let mut fallback = self.config.clone();
                fallback.buffer_size = cpal::BufferSize::Default;
                self.build(&fallback, renderer)
                    .map_err(|error| format!("{first_error}; default buffer: {error}"))?
            }
        };
        self.stream = Some(stream);
        if let Some(stream) = self.stream.as_ref() {
            stream.play().map_err(|error| error.to_string())?;
        }
        self.watchdog =
            CallbackWatchdog::new(self.failure.frames.load(Ordering::Relaxed), Instant::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_backend_returns_an_error_without_opening_devices() {
        assert!(OutputBackend::setup(BackendSettings { disabled: true }, 128).is_err());
    }

    #[test]
    fn runtime_failure_is_recorded_without_panicking() {
        let failure = Failure::default();
        assert_eq!(failure.message(), None);
        failure.record("Device disconnected".to_string());
        failure.record("Later failure".to_string());
        assert_eq!(failure.message().as_deref(), Some("Device disconnected"));
    }

    #[test]
    fn callback_watchdog_allows_startup_then_detects_no_progress() {
        let start = Instant::now();
        let mut watchdog = CallbackWatchdog::new(0, start);
        assert!(!watchdog.stalled(0, start + Duration::from_millis(1999), 48000));
        assert!(watchdog.stalled(0, start + CALLBACK_TIMEOUT, 48000));
    }

    #[test]
    fn pulse_watchdog_tolerates_two_second_sink_requests_and_detects_a_stall() {
        let start = Instant::now();
        let mut watchdog = CallbackWatchdog::with_timeout(0, start, Duration::from_secs(5));
        assert!(!watchdog.stalled(4800, start + Duration::from_millis(100), 48000));
        assert!(!watchdog.stalled(4800, start + Duration::from_secs(2), 48000));
        assert!(!watchdog.stalled(96000, start + Duration::from_millis(2100), 48000));
        assert!(!watchdog.stalled(96000, start + Duration::from_secs(4), 48000));
        assert!(!watchdog.stalled(192000, start + Duration::from_millis(4100), 48000));
        assert!(watchdog.stalled(192000, start + Duration::from_millis(9100), 48000));
    }

    #[test]
    fn callback_progress_renews_watchdog_even_after_a_long_poll_gap() {
        let start = Instant::now();
        let mut watchdog = CallbackWatchdog::new(0, start);
        let resumed = start + Duration::from_secs(60);
        let frames = 60 * 48000;
        assert!(!watchdog.stalled(frames, resumed, 48000));
        assert!(!watchdog.stalled(frames, resumed + Duration::from_millis(1999), 48000));
        assert!(watchdog.stalled(frames, resumed + CALLBACK_TIMEOUT, 48000));
    }

    #[test]
    fn occasional_callbacks_cannot_keep_a_slow_output_alive_forever() {
        let start = Instant::now();
        let mut watchdog = CallbackWatchdog::new(0, start);
        assert!(!watchdog.stalled(48000, start + Duration::from_secs(1), 48000));
        assert!(!watchdog.stalled(96000, start + Duration::from_secs(2), 48000));
        assert!(!watchdog.stalled(97024, start + Duration::from_secs(3), 48000));
        assert!(watchdog.stalled(98048, start + Duration::from_secs(4), 48000));
    }

    #[test]
    fn callback_watchdog_tolerates_bursts_and_scheduling_jitter() {
        let start = Instant::now();
        let mut watchdog = CallbackWatchdog::new(0, start);
        assert!(!watchdog.stalled(48000, start + Duration::from_millis(100), 48000));
        assert!(!watchdog.stalled(48000, start + Duration::from_millis(1900), 48000));
        assert!(!watchdog.stalled(72000, start + Duration::from_secs(2), 48000));
    }

    #[test]
    fn wsl_audio_uses_explicit_pulse_without_affecting_native_linux() {
        assert!(needs_wsl_pulse(
            Some("unix:/mnt/wslg/PulseServer"),
            false,
            false
        ));
        assert!(needs_wsl_pulse(None, true, false));
        assert!(needs_wsl_pulse(None, false, true));
        assert!(!needs_wsl_pulse(
            Some("unix:/run/user/1000/pulse/native"),
            false,
            false
        ));
        assert!(!needs_wsl_pulse(None, false, false));
    }

    #[test]
    fn opening_timeout_bounds_workers_and_drops_a_late_result() {
        static OPENING: AtomicBool = AtomicBool::new(false);
        struct LateResult(std::sync::mpsc::Sender<()>);
        impl Drop for LateResult {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let (resume, waiting) = std::sync::mpsc::channel();
        let (dropped, confirm_drop) = std::sync::mpsc::channel();
        let result = open_with_timeout(&OPENING, Duration::from_millis(10), move || {
            waiting.recv().unwrap();
            Ok(LateResult(dropped))
        });
        assert!(matches!(result, Err(error) if error.contains("timed out")));
        let second = open_with_timeout(&OPENING, Duration::from_secs(1), || Ok(()));
        assert!(matches!(second, Err(error) if error.contains("still pending")));
        resume.send(()).unwrap();
        confirm_drop.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn opening_worker_returns_success_and_creation_errors() {
        static SUCCESS: AtomicBool = AtomicBool::new(false);
        static FAILURE: AtomicBool = AtomicBool::new(false);
        assert_eq!(
            open_with_timeout(&SUCCESS, Duration::from_secs(1), || Ok(42)),
            Ok(42)
        );
        assert_eq!(
            open_with_timeout::<(), _>(
                &FAILURE,
                Duration::from_secs(1),
                || Err("No server".into())
            ),
            Err("No server".into())
        );
    }
}
