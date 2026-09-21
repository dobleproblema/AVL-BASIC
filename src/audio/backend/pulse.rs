//! Rust PulseAudio output for WSLg, driven by the server's PCM requests.
//!
//! Keep the stream target and minimum request independent: CPAL's fixed Pulse
//! buffers select a zero device latency on RDPSink, causing excessive wakeups.
//! This adapter negotiates 100 ms of stream buffering and an 80 ms sink latency.
// Wire protocol sequence based on pulseaudio-rs examples/playback.rs:
// Copyright 2023 Colin Marc, MIT; see licenses/pulseaudio-MIT.txt.
use super::Failure;
use kira::backend::Renderer;
use pulseaudio::protocol;
use std::ffi::CString;
use std::io::{BufReader, Cursor, Read};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub(super) const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const FRAME_BYTES: u32 = 4;
const TARGET_BYTES: u32 = SAMPLE_RATE * FRAME_BYTES / 10;
const MAX_BUFFER_BYTES: u32 = SAMPLE_RATE * FRAME_BYTES;
const MAX_PACKET_BYTES: usize = 64 * 1024;
const CHUNK_FRAMES: usize = 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

fn buffer_attributes() -> protocol::stream::BufferAttr {
    protocol::stream::BufferAttr {
        max_length: MAX_BUFFER_BYTES,
        target_length: TARGET_BYTES,
        minimum_request_length: SAMPLE_RATE * FRAME_BYTES / 100,
        ..Default::default()
    }
}

fn validate_request(bytes: u32) -> Result<(), String> {
    if bytes % FRAME_BYTES != 0 || bytes > TARGET_BYTES {
        return Err(format!(
            "PulseAudio requested an invalid PCM block ({bytes} bytes)"
        ));
    }
    Ok(())
}

fn validate_buffer(attributes: protocol::stream::BufferAttr) -> Result<(), String> {
    if attributes.target_length == 0
        || attributes.target_length > TARGET_BYTES
        || attributes.max_length < attributes.target_length
        || attributes.max_length > MAX_BUFFER_BYTES
        || attributes.pre_buffering > attributes.target_length
        || attributes.minimum_request_length == 0
        || attributes.minimum_request_length > attributes.target_length
        || [
            attributes.target_length,
            attributes.max_length,
            attributes.pre_buffering,
            attributes.minimum_request_length,
        ]
        .iter()
        .any(|bytes| bytes % FRAME_BYTES != 0)
    {
        return Err("PulseAudio negotiated unsupported playback buffering".into());
    }
    Ok(())
}

struct Connection {
    socket: BufReader<UnixStream>,
    packet: Vec<u8>,
    version: u16,
    channel: u32,
    initial_bytes: u32,
}

struct PlaybackReply {
    channel: u32,
    requested_bytes: u32,
    buffer_attr: protocol::stream::BufferAttr,
    sample_spec: protocol::SampleSpec,
    channel_map: protocol::ChannelMap,
    suspended: bool,
}

fn protocol_error(error: protocol::ProtocolError) -> String {
    format!("PulseAudio protocol: {error}")
}

impl Connection {
    fn new(socket: UnixStream, timeout: Duration) -> Result<Self, String> {
        socket
            .set_read_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        socket
            .set_write_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            socket: BufReader::new(socket),
            packet: vec![0; MAX_PACKET_BYTES],
            version: protocol::MAX_VERSION,
            channel: 0,
            initial_bytes: 0,
        })
    }

    // Read the entire bounded packet before decoding it. In particular, an
    // unknown message cannot leave a payload behind and desynchronize framing.
    // Any timeout (including a partial packet) closes this connection.
    fn read_packet(&mut self) -> Result<usize, String> {
        let descriptor = protocol::read_descriptor(&mut self.socket)
            .map_err(|error| format!("PulseAudio header: {error}"))?;
        if descriptor.channel != u32::MAX
            || !descriptor.flags.is_empty()
            || descriptor.length == 0
            || descriptor.length as usize > self.packet.len()
        {
            return Err("PulseAudio sent an invalid control packet".into());
        }
        let length = descriptor.length as usize;
        self.socket
            .read_exact(&mut self.packet[..length])
            .map_err(|error| format!("PulseAudio packet: {error}"))?;
        Ok(length)
    }

    fn command(&mut self, sequence: u32, command: protocol::Command) -> Result<(), String> {
        protocol::write_command_message(self.socket.get_mut(), sequence, &command, self.version)
            .map_err(|error| format!("PulseAudio command: {error}"))
    }

    fn reply_with<R>(
        &mut self,
        expected: u32,
        read: impl FnOnce(&mut protocol::TagStructReader<'_>) -> Result<R, protocol::ProtocolError>,
    ) -> Result<R, String> {
        let length = self.read_packet()?;
        let mut cursor = Cursor::new(&self.packet[..length]);
        let mut reader = protocol::TagStructReader::new(&mut cursor, self.version);
        let command = reader
            .read_enum::<protocol::CommandTag>()
            .map_err(protocol_error)?;
        let sequence = reader.read_u32().map_err(protocol_error)?;
        if sequence != expected {
            return Err("PulseAudio sent an unexpected reply sequence".into());
        }
        match command {
            protocol::CommandTag::Reply => read(&mut reader).map_err(protocol_error),
            protocol::CommandTag::Error => {
                let error = reader
                    .read_enum::<protocol::PulseError>()
                    .map_err(protocol_error)?;
                Err(format!("PulseAudio server: {error:?}"))
            }
            _ => Err("PulseAudio sent an unexpected handshake message".into()),
        }
    }

    fn reply<R: protocol::CommandReply>(&mut self, expected: u32) -> Result<R, String> {
        self.reply_with(expected, |reader| reader.read::<R>())
    }

    fn playback_reply(&mut self) -> Result<PlaybackReply, String> {
        self.reply_with(2, |reader| {
            let channel = reader
                .read_index()?
                .ok_or_else(|| protocol::ProtocolError::Invalid("invalid stream channel".into()))?;
            let _stream_index = reader.read_index()?;
            let requested_bytes = reader.read_u32()?;
            let buffer_attr = protocol::stream::BufferAttr {
                max_length: reader.read_u32()?,
                target_length: reader.read_u32()?,
                pre_buffering: reader.read_u32()?,
                minimum_request_length: reader.read_u32()?,
                ..Default::default()
            };
            let sample_spec = reader.read()?;
            let channel_map = reader.read()?;
            let _sink_index = reader.read_index()?;
            let _sink_name = reader.read_string()?;
            let suspended = reader.read_bool()?;
            let _sink_latency = reader.read_usec()?;
            // FormatInfo/Props extensions are irrelevant to our fixed PCM format.
            // Do not decode arbitrary blobs whose internal length fields could
            // allocate independently of the already bounded packet size.
            Ok(PlaybackReply {
                channel,
                requested_bytes,
                buffer_attr,
                sample_spec,
                channel_map,
                suspended,
            })
        })
    }

    fn initialize(&mut self, cookie: Vec<u8>) -> Result<(), String> {
        self.command(
            0,
            protocol::Command::Auth(protocol::AuthParams {
                version: protocol::MAX_VERSION,
                supports_shm: false,
                supports_memfd: false,
                cookie,
            }),
        )?;
        let auth = self.reply::<protocol::AuthReply>(0)?;
        if auth.version < protocol::MIN_VERSION {
            return Err("PulseAudio server protocol is too old".into());
        }
        self.version = auth.version.min(protocol::MAX_VERSION);
        let mut properties = protocol::Props::new();
        properties.set(
            protocol::Prop::ApplicationName,
            CString::new("AVL BASIC").unwrap(),
        );
        properties.set(
            protocol::Prop::ApplicationProcessId,
            CString::new(std::process::id().to_string()).unwrap(),
        );
        self.command(1, protocol::Command::SetClientName(properties))?;
        self.reply::<protocol::SetClientNameReply>(1)?;
        self.command(
            2,
            protocol::Command::CreatePlaybackStream(protocol::PlaybackStreamParams {
                sample_spec: protocol::SampleSpec {
                    format: protocol::SampleFormat::S16Le,
                    channels: CHANNELS as u8,
                    sample_rate: SAMPLE_RATE,
                },
                channel_map: protocol::ChannelMap::stereo(),
                cvolume: Some(protocol::ChannelVolume::norm(CHANNELS as u8)),
                sink_name: Some(protocol::DEFAULT_SINK.to_owned()),
                buffer_attr: buffer_attributes(),
                flags: protocol::stream::StreamFlags {
                    adjust_latency: false,
                    ..Default::default()
                },
                ..Default::default()
            }),
        )?;
        let info = self.playback_reply()?;
        if info.sample_spec.format != protocol::SampleFormat::S16Le
            || info.sample_spec.channels != CHANNELS as u8
            || info.sample_spec.sample_rate != SAMPLE_RATE
            || info.channel_map != protocol::ChannelMap::stereo()
            || info.suspended
        {
            return Err("PulseAudio negotiated an unsupported or suspended output".into());
        }
        validate_buffer(info.buffer_attr)?;
        validate_request(info.requested_bytes)?;
        self.channel = info.channel;
        self.initial_bytes = info.requested_bytes;
        Ok(())
    }

    fn next_request(&mut self) -> Result<Option<u32>, String> {
        let length = self.read_packet()?;
        let mut cursor = Cursor::new(&self.packet[..length]);
        let mut reader = protocol::TagStructReader::new(&mut cursor, self.version);
        let command = reader
            .read_enum::<protocol::CommandTag>()
            .map_err(protocol_error)?;
        let _sequence = reader.read_u32().map_err(protocol_error)?;
        match command {
            protocol::CommandTag::Request => {
                let request = reader.read::<protocol::Request>().map_err(protocol_error)?;
                if request.channel != self.channel {
                    return Err("PulseAudio requested an unknown output stream".into());
                }
                validate_request(request.length)?;
                Ok(Some(request.length))
            }
            protocol::CommandTag::Error => {
                let error = reader
                    .read_enum::<protocol::PulseError>()
                    .map_err(protocol_error)?;
                Err(format!("PulseAudio server: {error:?}"))
            }
            protocol::CommandTag::PlaybackStreamKilled => {
                Err("PulseAudio output was removed".into())
            }
            protocol::CommandTag::Overflow => Err("PulseAudio output buffer overflowed".into()),
            protocol::CommandTag::PlaybackStreamSuspended => {
                let state = reader
                    .read::<protocol::StreamSuspendedParams>()
                    .map_err(protocol_error)?;
                if state.suspended {
                    Err("PulseAudio output was suspended".into())
                } else {
                    Ok(None)
                }
            }
            protocol::CommandTag::PlaybackStreamMoved => {
                let state = reader
                    .read::<protocol::PlaybackStreamMovedParams>()
                    .map_err(protocol_error)?;
                if state.device_suspended {
                    return Err("PulseAudio output moved to a suspended device".into());
                }
                validate_buffer(state.buffer_attr)?;
                Ok(None)
            }
            protocol::CommandTag::PlaybackBufferAttrChanged => {
                let state = reader
                    .read::<protocol::PlaybackBufferAttrChanged>()
                    .map_err(protocol_error)?;
                validate_buffer(state.buffer_attr)?;
                Ok(None)
            }
            // An underrun can recover with the next server request. Progress is
            // also watched by OutputBackend, so a silent server cannot freeze BASIC.
            protocol::CommandTag::Underflow => {
                reader
                    .read::<protocol::Underflow>()
                    .map_err(protocol_error)?;
                Ok(None)
            }
            protocol::CommandTag::Started => {
                reader.read_u32().map_err(protocol_error)?;
                Ok(None)
            }
            _ => Err("PulseAudio sent an unexpected playback event".into()),
        }
    }

    fn serve(
        mut self,
        mut renderer: Renderer,
        stop: &AtomicBool,
        failure: &Failure,
    ) -> Result<(), String> {
        let mut float_buffer = [0.0_f32; CHUNK_FRAMES * CHANNELS as usize];
        let mut pcm_buffer = [0_u8; CHUNK_FRAMES * FRAME_BYTES as usize];
        let mut requested = self.initial_bytes;
        loop {
            if stop.load(Ordering::Acquire) || failure.failed.load(Ordering::Acquire) {
                return Ok(());
            }
            if requested != 0 {
                renderer.on_start_processing();
                let mut frames_left = (requested / FRAME_BYTES) as usize;
                while frames_left != 0 {
                    if stop.load(Ordering::Acquire) || failure.failed.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    let frames = frames_left.min(CHUNK_FRAMES);
                    let samples = &mut float_buffer[..frames * CHANNELS as usize];
                    renderer.process(samples, CHANNELS);
                    let pcm = &mut pcm_buffer[..frames * FRAME_BYTES as usize];
                    encode_pcm(samples, pcm);
                    protocol::write_memblock(self.socket.get_mut(), self.channel, pcm, 0)
                        .map_err(|error| format!("PulseAudio PCM write: {error}"))?;
                    failure.frames.fetch_add(frames as u64, Ordering::Relaxed);
                    frames_left -= frames;
                }
            }
            requested = self.next_request()?.unwrap_or(0);
        }
    }
}

fn encode_pcm(samples: &[f32], bytes: &mut [u8]) {
    for (sample, bytes) in samples.iter().zip(bytes.chunks_exact_mut(2)) {
        // Rust's float-to-int cast maps NaN to zero; clamp infinities and the
        // positive endpoint to the signed PCM range as well.
        let sample = (sample.clamp(-1.0, 1.0) * 32768.0)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        bytes.copy_from_slice(&sample.to_le_bytes());
    }
}

pub(super) struct PulseOutput {
    connection: Option<Connection>,
    shutdown_socket: UnixStream,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PulseOutput {
    pub(super) fn connect() -> Result<Self, String> {
        let path = pulseaudio::socket_path_from_env().ok_or("PulseAudio server is unavailable")?;
        let socket = UnixStream::connect(path)
            .map_err(|error| format!("Cannot connect to PulseAudio: {error}"))?;
        let shutdown_socket = socket.try_clone().map_err(|error| error.to_string())?;
        let mut connection = Connection::new(socket, IO_TIMEOUT)?;
        let cookie = pulseaudio::cookie_path_from_env()
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_default();
        connection.initialize(cookie)?;
        Ok(Self {
            connection: Some(connection),
            shutdown_socket,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
        })
    }

    pub(super) fn start(
        &mut self,
        renderer: Renderer,
        failure: Arc<Failure>,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .take()
            .ok_or("PulseAudio output was already started")?;
        let stop = self.stop.clone();
        let worker = thread::Builder::new()
            .name("avl-pulse-output".into())
            .spawn(move || {
                if let Err(error) = connection.serve(renderer, &stop, &failure) {
                    if !stop.load(Ordering::Acquire) {
                        failure.record(error);
                    }
                }
            })
            .map_err(|error| format!("Cannot start PulseAudio worker: {error}"))?;
        self.worker = Some(worker);
        Ok(())
    }

    pub(super) fn is_started(&self) -> bool {
        self.worker.is_some()
    }
}

impl Drop for PulseOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Closing the socket cancels reads/writes and discards queued PCM.
        // Do not drain: STOP/NEW/RUN must not wait for sound to finish playing.
        let _ = self.shutdown_socket.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc;
    use std::time::Instant;

    fn pair(timeout: Duration) -> (Connection, UnixStream) {
        let (client, server) = UnixStream::pair().unwrap();
        (Connection::new(client, timeout).unwrap(), server)
    }

    fn descriptor(server: &mut UnixStream, channel: u32, length: u32) {
        protocol::write_descriptor(
            server,
            &protocol::Descriptor {
                channel,
                length,
                offset: 0,
                flags: protocol::DescriptorFlags::empty(),
            },
        )
        .unwrap();
    }

    #[test]
    fn pulse_buffer_leaves_a_nonzero_sink_latency_and_limits_lookahead() {
        let mut attributes = buffer_attributes();
        assert_eq!(attributes.target_length / FRAME_BYTES, SAMPLE_RATE / 10);
        assert_eq!(
            attributes.minimum_request_length / FRAME_BYTES,
            SAMPLE_RATE / 100
        );
        assert_eq!(
            (attributes.target_length - 2 * attributes.minimum_request_length) / FRAME_BYTES,
            SAMPLE_RATE * 80 / 1000
        );
        attributes.pre_buffering = attributes.target_length;
        assert!(validate_buffer(attributes).is_ok());
        attributes.target_length += FRAME_BYTES;
        assert!(validate_buffer(attributes).is_err());
        assert!(validate_request(0).is_ok());
        assert!(validate_request(TARGET_BYTES).is_ok());
        assert!(validate_request(TARGET_BYTES + FRAME_BYTES).is_err());
        assert!(validate_request(1).is_err());
    }

    #[test]
    fn pulse_validates_packet_limits_before_reading_or_decoding_payload() {
        for (channel, length) in [
            (0, 8),
            (u32::MAX, 0),
            (u32::MAX, MAX_PACKET_BYTES as u32 + 1),
        ] {
            let (mut connection, mut server) = pair(Duration::from_secs(1));
            descriptor(&mut server, channel, length);
            assert!(connection
                .read_packet()
                .unwrap_err()
                .contains("invalid control packet"));
        }
    }

    #[test]
    fn pulse_request_requires_frame_alignment_channel_and_bounded_lookahead() {
        for (channel, length, accepted) in [
            (0, TARGET_BYTES, true),
            (0, 3, false),
            (0, TARGET_BYTES + FRAME_BYTES, false),
            (1, FRAME_BYTES, false),
        ] {
            let (mut connection, mut server) = pair(Duration::from_secs(1));
            protocol::write_command_message(
                &mut server,
                99,
                &protocol::Command::Request(protocol::Request { channel, length }),
                protocol::MAX_VERSION,
            )
            .unwrap();
            let result = connection.next_request();
            if accepted {
                assert_eq!(result.unwrap(), Some(length));
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn pulse_shutdown_interrupts_a_pending_packet_read() {
        let (mut connection, _server) = pair(Duration::from_secs(2));
        let cancel = connection.socket.get_ref().try_clone().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            started_tx.send(()).unwrap();
            done_tx.send(connection.read_packet()).unwrap();
        });
        started_rx.recv().unwrap();
        cancel.shutdown(Shutdown::Both).unwrap();
        assert!(done_rx
            .recv_timeout(Duration::from_millis(500))
            .unwrap()
            .is_err());
        worker.join().unwrap();
    }

    #[test]
    fn pulse_partial_packet_timeout_is_a_connection_error() {
        let (mut connection, mut server) = pair(Duration::from_millis(20));
        descriptor(&mut server, u32::MAX, 20);
        server.write_all(&[0]).unwrap();
        let started = Instant::now();
        assert!(connection
            .read_packet()
            .unwrap_err()
            .contains("PulseAudio packet"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn pulse_handshake_rejects_oversized_packets_too() {
        let (mut connection, mut server) = pair(Duration::from_secs(1));
        descriptor(&mut server, u32::MAX, u32::MAX);
        assert!(connection
            .reply::<protocol::AuthReply>(0)
            .unwrap_err()
            .contains("invalid control packet"));
    }

    #[test]
    fn pulse_handshake_validates_version_and_negotiated_pcm() {
        let (mut old, mut old_server) = pair(Duration::from_secs(1));
        protocol::write_reply_message(
            &mut old_server,
            0,
            &protocol::AuthReply {
                version: protocol::MIN_VERSION - 1,
                ..Default::default()
            },
            protocol::MAX_VERSION,
        )
        .unwrap();
        assert!(old.initialize(Vec::new()).unwrap_err().contains("too old"));

        for rate in [SAMPLE_RATE, SAMPLE_RATE / 2] {
            let (mut connection, mut server) = pair(Duration::from_secs(1));
            protocol::write_reply_message(
                &mut server,
                0,
                &protocol::AuthReply {
                    version: protocol::MAX_VERSION,
                    ..Default::default()
                },
                protocol::MAX_VERSION,
            )
            .unwrap();
            protocol::write_reply_message(
                &mut server,
                1,
                &protocol::SetClientNameReply { client_id: 1 },
                protocol::MAX_VERSION,
            )
            .unwrap();
            let mut attributes = buffer_attributes();
            attributes.pre_buffering = TARGET_BYTES;
            let reply = protocol::CreatePlaybackStreamReply {
                requested_bytes: TARGET_BYTES,
                sample_spec: protocol::SampleSpec {
                    format: protocol::SampleFormat::S16Le,
                    channels: 2,
                    sample_rate: rate,
                },
                channel_map: protocol::ChannelMap::stereo(),
                buffer_attr: attributes,
                ..Default::default()
            };
            protocol::write_reply_message(&mut server, 2, &reply, protocol::MAX_VERSION).unwrap();
            let result = connection.initialize(Vec::new());
            if rate == SAMPLE_RATE {
                result.unwrap();
                assert_eq!(connection.initial_bytes, TARGET_BYTES);
            } else {
                assert!(result.unwrap_err().contains("unsupported"));
            }
        }
    }

    #[test]
    fn pulse_ignores_unused_format_extensions_without_losing_packet_framing() {
        let (mut connection, mut server) = pair(Duration::from_secs(1));
        let reply = protocol::CreatePlaybackStreamReply::default();
        let mut encoded = Vec::new();
        protocol::write_reply_message(&mut encoded, 2, &reply, protocol::MAX_VERSION).unwrap();
        // An unneeded extension with a deliberately impossible blob length must
        // neither allocate that blob nor become the next packet's header.
        encoded.extend_from_slice(&[b'x', 255, 255, 255, 255]);
        let length = (encoded.len() - protocol::DESCRIPTOR_SIZE) as u32;
        encoded[..4].copy_from_slice(&length.to_be_bytes());
        server.write_all(&encoded).unwrap();
        protocol::write_command_message(
            &mut server,
            99,
            &protocol::Command::Request(protocol::Request {
                channel: 0,
                length: FRAME_BYTES,
            }),
            protocol::MAX_VERSION,
        )
        .unwrap();
        connection.playback_reply().unwrap();
        assert_eq!(connection.next_request().unwrap(), Some(FRAME_BYTES));
    }

    #[test]
    fn pulse_removed_stream_reports_failure() {
        let (mut connection, mut server) = pair(Duration::from_secs(1));
        protocol::write_command_message(
            &mut server,
            99,
            &protocol::Command::PlaybackStreamKilled(0),
            protocol::MAX_VERSION,
        )
        .unwrap();
        assert!(connection.next_request().unwrap_err().contains("removed"));
    }

    #[test]
    fn pulse_pcm_conversion_clamps_and_preserves_stereo_order() {
        let samples = [
            f32::NEG_INFINITY,
            -1.0,
            -0.5,
            0.0,
            0.5,
            1.0,
            f32::INFINITY,
            f32::NAN,
        ];
        let mut bytes = [0; 16];
        encode_pcm(&samples, &mut bytes);
        let converted: Vec<_> = bytes
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(
            converted,
            [-32768, -32768, -16384, 0, 16384, 32767, 32767, 0]
        );
    }
}
