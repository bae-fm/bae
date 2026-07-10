use super::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioLockMissLog, AudioOutput, AudioOutputControls,
    AudioState, AudioStream,
};
use crate::playback::source::PlaybackSource;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleRate, Stream, StreamConfig};
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{error, info};

// -- cpal::Stream implements AudioStream --

impl AudioStream for Stream {
    fn play(&self) -> Result<(), AudioError> {
        cpal::traits::StreamTrait::play(self)
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))
    }
}

/// Audio output using the system audio device via CPAL.
pub struct CpalAudioOutput {
    controls: AudioOutputControls,
    /// macOS default-output-device change listener, present only on the main
    /// player's output (`with_device_listener`). Held for its `Drop`, which
    /// unregisters the CoreAudio property listener when the output goes away.
    /// `None` on the preview player's output (no listener) and on non-macOS.
    #[cfg(target_os = "macos")]
    _device_listener: Option<device_listener::DefaultDeviceListener>,
}

impl CpalAudioOutput {
    /// Build a cpal output with no default-device change listener — the preview
    /// player's output, and the main player's output on non-macOS. Verifies a
    /// default output device exists up front so construction fails fast when
    /// there's no audio hardware. Without a listener, an output switch takes
    /// effect at the next stream rebuild (stop / format change).
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        host.default_output_device()
            .ok_or(AudioError::DeviceNotFound)?;

        Ok(Self {
            controls: AudioOutputControls::new(1.0),
            #[cfg(target_os = "macos")]
            _device_listener: None,
        })
    }

    /// Build the main player's cpal output with a CoreAudio listener that runs
    /// `on_default_device_changed` whenever the system default output device
    /// changes. The service passes a closure that dispatches `OutputDeviceChanged`,
    /// so the persistent stream rebuilds onto the new default. macOS-only: other
    /// platforms have no such listener and follow the output at the next rebuild.
    #[cfg(target_os = "macos")]
    pub fn with_device_listener(
        on_default_device_changed: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        host.default_output_device()
            .ok_or(AudioError::DeviceNotFound)?;

        let device_listener =
            device_listener::DefaultDeviceListener::register(Box::new(on_default_device_changed))?;

        Ok(Self {
            controls: AudioOutputControls::new(1.0),
            _device_listener: Some(device_listener),
        })
    }
}

impl AudioOutput for CpalAudioOutput {
    fn create_stream(
        &mut self,
        source: Arc<Mutex<PlaybackSource>>,
        source_sample_rate: u32,
        source_channels: u32,
        audio_events: AudioEventSender,
        position_update_interval_ms: u32,
    ) -> Result<Box<dyn AudioStream>, AudioError> {
        // Resolve the current default output device at build time. The device is
        // deliberately not cached at construction: caching pins to whatever was
        // default at startup, and on macOS cpal sets the audio unit's
        // `CurrentDevice` to that cached id on each build, so a rebuild would bind
        // the stale device instead of the current default.
        //
        // With the persistent output stream this build runs only on stop, a format
        // change, and a default-device change — not per track — so switching the
        // system output takes effect via the macOS default-device listener, which
        // dispatches `OutputDeviceChanged` and rebuilds this stream onto the new
        // default. On other platforms a switch takes effect at the next rebuild
        // (stop / format change).
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::DeviceNotFound)?;
        let default_config = device
            .default_output_config()
            .map_err(|e| AudioError::StreamConfigError(e.to_string()))?;
        let mut stream_config = StreamConfig::from(default_config);
        stream_config.sample_rate = SampleRate(source_sample_rate);
        stream_config.channels = source_channels as u16;

        info!(
            device = ?device.name(),
            sample_rate = source_sample_rate,
            channels = source_channels,
            "Building audio output stream"
        );

        let mut drain = AudioDrain::new(
            self.controls.clone(),
            source,
            audio_events,
            position_update_interval_ms,
        );
        let mut lock_miss = AudioLockMissLog::new();

        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let _ = drain.drain_iteration(data, true, true, Some(&mut lock_miss));
                },
                |err| {
                    error!("Streaming audio error: {:?}", err);
                },
                None,
            )
            .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        Ok(Box::new(stream))
    }

    fn set_state(&self, new_state: AudioState) {
        self.controls.set_state(new_state);
    }

    fn get_state(&self) -> AudioState {
        self.controls.get_state()
    }

    fn set_volume(&self, volume: f32) {
        self.controls.set_volume(volume);
    }

    fn get_volume(&self) -> f32 {
        self.controls.get_volume()
    }
}

/// CoreAudio default-output-device change listener (macOS). Registers a property
/// listener on the system object's `kAudioHardwarePropertyDefaultOutputDevice`;
/// the listener fires on a CoreAudio thread and runs the registered callback,
/// which does nothing but dispatch an internal command — the actual stream
/// rebuild happens on the service's command loop.
#[cfg(target_os = "macos")]
mod device_listener {
    use super::AudioError;
    use coreaudio_sys::{
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectAddPropertyListener,
        AudioObjectID, AudioObjectPropertyAddress, AudioObjectRemovePropertyListener, OSStatus,
    };
    use std::os::raw::c_void;

    type Callback = Box<dyn Fn() + Send + Sync>;

    /// A registered CoreAudio property listener. Owns the boxed callback (kept
    /// alive for as long as the listener is registered) and removes the listener
    /// and frees the callback on drop.
    pub(super) struct DefaultDeviceListener {
        /// Raw pointer to the boxed callback handed to CoreAudio as client data.
        /// Freed in `Drop` once the listener is removed and no CoreAudio thread
        /// can reach it.
        callback: *mut Callback,
        address: AudioObjectPropertyAddress,
    }

    // The raw callback pointer is only dereferenced by CoreAudio's listener thread
    // (via `listener_proc`) and freed on drop after the listener is removed; the
    // boxed `Fn` is `Send + Sync`, so the handle is safe to move to the service
    // thread that owns the output.
    unsafe impl Send for DefaultDeviceListener {}

    /// CoreAudio invokes this on its own thread when the default output device
    /// changes. It only runs the callback (which dispatches an internal command);
    /// no CoreAudio or audio work happens here.
    extern "C" fn listener_proc(
        _object: AudioObjectID,
        _num_addresses: u32,
        _addresses: *const AudioObjectPropertyAddress,
        client_data: *mut c_void,
    ) -> OSStatus {
        // SAFETY: `client_data` is the `*mut Callback` we registered; it outlives
        // the listener (freed only in `Drop`, after the listener is removed).
        let callback = unsafe { &*(client_data as *const Callback) };
        callback();
        0
    }

    impl DefaultDeviceListener {
        pub(super) fn register(callback: Callback) -> Result<Self, AudioError> {
            let callback = Box::into_raw(Box::new(callback));
            let address = AudioObjectPropertyAddress {
                mSelector: kAudioHardwarePropertyDefaultOutputDevice,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain,
            };
            // SAFETY: `kAudioObjectSystemObject` is always a valid object;
            // `listener_proc` is a valid `extern "C"` listener; the callback
            // pointer outlives the registration (freed in `Drop`).
            let status = unsafe {
                AudioObjectAddPropertyListener(
                    kAudioObjectSystemObject,
                    &address,
                    Some(listener_proc),
                    callback as *mut c_void,
                )
            };
            if status != 0 {
                // Registration failed, so nothing will ever call the callback —
                // reclaim the box we couldn't hand off.
                drop(unsafe { Box::from_raw(callback) });
                return Err(AudioError::StreamBuildError(format!(
                    "AudioObjectAddPropertyListener failed with OSStatus {status}"
                )));
            }
            Ok(Self { callback, address })
        }
    }

    impl Drop for DefaultDeviceListener {
        fn drop(&mut self) {
            // SAFETY: remove the exact listener we registered. We deliberately do
            // NOT free the callback box afterward: AudioObjectRemovePropertyListener
            // does not guarantee that an in-flight `listener_proc` invocation on a
            // CoreAudio thread has returned by the time it returns, so freeing here
            // would be an unsynchronizable use-after-free. The output lives for the
            // process, so this drops at most once at shutdown — one small
            // intentionally-leaked allocation is the correct trade against the race.
            let status = unsafe {
                AudioObjectRemovePropertyListener(
                    kAudioObjectSystemObject,
                    &self.address,
                    Some(listener_proc),
                    self.callback as *mut c_void,
                )
            };
            if status != 0 {
                tracing::warn!(
                    "AudioObjectRemovePropertyListener failed with OSStatus {status}; \
                     the default-device listener may keep firing until process exit"
                );
            }
        }
    }
}
