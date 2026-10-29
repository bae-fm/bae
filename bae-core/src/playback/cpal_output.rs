use super::audio_output::{
    AudioDrain, AudioError, AudioEventSender, AudioLockMissLog, AudioOutput, AudioOutputControls,
    AudioStream,
};
use crate::playback::source::PlaybackSource;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, Stream, StreamConfig};
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{error, info};

// -- cpal::Stream implements AudioStream --

impl AudioStream for Stream {
    fn play(&self) -> Result<(), AudioError> {
        cpal::traits::StreamTrait::play(self).map_err(|error| match error.kind() {
            cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::DeviceBusy => {
                AudioError::DeviceUnavailable
            }
            _ => AudioError::Unexpected(format!("starting the stream: {error}")),
        })
    }
}

/// Audio output using the system audio device via CPAL.
pub struct CpalAudioOutput {
    controls: AudioOutputControls,
}

impl CpalAudioOutput {
    /// Build a cpal output. Verifies a default output device exists up front so
    /// construction fails fast when there's no audio hardware. Which device the
    /// samples land on is resolved per stream build, so an output switch takes
    /// effect at the next rebuild — on macOS and Windows the service forces one
    /// by watching the default device (see `watch_default_output_device`).
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        host.default_output_device()
            .ok_or(AudioError::DeviceUnavailable)?;

        Ok(Self {
            controls: AudioOutputControls::new(1.0),
        })
    }
}

/// Watch the system default output device and run `on_change` whenever it
/// changes. The service registers this once for its lifetime and passes a
/// closure that dispatches `OutputDeviceChanged`, so the persistent stream
/// rebuilds onto the new default; the returned guard unregisters on drop.
///
/// This watches the *system*, not any one output, which is why it lives beside
/// the outputs rather than inside one — the service opens two (main and
/// preview) and exactly one listener may exist. macOS and Windows only: there a
/// cpal stream stays bound to the device it opened on, so without a rebuild the
/// samples keep going to the old device.
///
/// Linux needs no watch. cpal opens ALSA's `default` device, which on a
/// PipeWire or PulseAudio system is the sound server's ALSA plugin, and the
/// stream it creates names no sink. The sound server moves such a stream to the
/// new default sink itself: WirePlumber relinks streams without a target when
/// `default.audio.sink` changes, and PulseAudio 14 and later move the streams on
/// the old default sink to the new one. With ALSA alone, `default` is fixed
/// configuration rather than a device that changes.
///
/// iOS needs none either. cpal's iOS stream is a RemoteIO unit, which plays on
/// the audio session's current route: the system changes the route itself when
/// headphones, a Bluetooth device or AirPlay come or go, and playback continues
/// on the new one. RemoteIO converts the stream's sample rate to the new
/// hardware's (Apple QA1777), so the stream bae built still fits.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn watch_default_output_device(
    on_change: impl Fn() + Send + Sync + 'static,
) -> Result<device_listener::DefaultDeviceListener, AudioError> {
    device_listener::DefaultDeviceListener::register(Box::new(on_change))
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
        // Resolve the default output device at each build. On macOS cpal sets
        // the stream's sample rate on the device it resolved as the default, so
        // a device kept from an earlier build would set the rate of whatever was
        // default then, not of the device that plays.
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::DeviceUnavailable)?;
        let stream_config = StreamConfig {
            channels: source_channels as u16,
            sample_rate: source_sample_rate,
            buffer_size: BufferSize::Default,
        };

        info!(
            device = ?device.description().map(|description| description.name().to_owned()),
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
                stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let _ = drain.drain_iteration(data, true, true, Some(&mut lock_miss));
                },
                |err| {
                    error!("Streaming audio error: {:?}", err);
                },
                None,
            )
            .map_err(|error| match error.kind() {
                cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::DeviceBusy => {
                    AudioError::DeviceUnavailable
                }
                cpal::ErrorKind::UnsupportedConfig => AudioError::UnsupportedFormat {
                    sample_rate: source_sample_rate,
                    channels: source_channels,
                },
                _ => AudioError::Unexpected(format!("building the stream: {error}")),
            })?;

        Ok(Box::new(stream))
    }

    fn controls(&self) -> &AudioOutputControls {
        &self.controls
    }
}

/// CoreAudio default-output-device change listener (macOS). Registers a property
/// listener on the system object's `kAudioHardwarePropertyDefaultOutputDevice`;
/// the listener fires on a CoreAudio thread and runs the registered callback,
/// which does nothing but dispatch an internal command — the actual stream
/// rebuild happens on the service's command loop.
#[cfg(target_os = "macos")]
pub(crate) mod device_listener {
    use super::AudioError;
    use coreaudio_sys::{
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectAddPropertyListener,
        AudioObjectID, AudioObjectPropertyAddress, AudioObjectRemovePropertyListener, OSStatus,
    };
    use std::os::raw::c_void;

    type Callback = Box<dyn Fn() + Send + Sync>;

    /// A registered CoreAudio property listener. Removes the listener on drop but
    /// deliberately leaks the boxed callback — see `Drop` for why.
    pub(crate) struct DefaultDeviceListener {
        /// Raw pointer to the boxed callback handed to CoreAudio as client data.
        /// Never freed: an in-flight `listener_proc` on a CoreAudio thread could
        /// still be reading it (see `Drop`).
        callback: *mut Callback,
        address: AudioObjectPropertyAddress,
    }

    // The raw callback pointer is only dereferenced by CoreAudio's listener thread
    // (via `listener_proc`) and is never freed; the boxed `Fn` is `Send + Sync`, so
    // the handle is safe to move to the service thread that owns the output.
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
        // SAFETY: `client_data` is the `*mut Callback` we registered, and it is
        // never freed while the process lives.
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
            // pointer outlives the registration (it is never freed).
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
                return Err(AudioError::Unexpected(format!(
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

/// Windows default-output-device change listener. Registers an
/// `IMMNotificationClient` with the MMDevice enumerator; Windows calls
/// `OnDefaultDeviceChanged` on a thread of its own for every flow and role, and
/// the client runs the registered callback only for the device cpal opens — the
/// default render endpoint for the console role
/// (`GetDefaultAudioEndpoint(eRender, eConsole)`). The callback does nothing but
/// dispatch an internal command, which keeps the notification non-blocking as
/// the interface requires; the rebuild happens on the service's command loop.
#[cfg(target_os = "windows")]
pub(crate) mod device_listener {
    use super::AudioError;
    use windows::core::{implement, PCWSTR};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
        IMMNotificationClient_Impl, MMDeviceEnumerator, DEVICE_STATE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

    type Callback = Box<dyn Fn() + Send + Sync>;

    /// The notification client Windows calls. Only a change of the default
    /// console render endpoint reaches the callback.
    #[implement(IMMNotificationClient)]
    struct DefaultOutputNotifications {
        on_change: Callback,
    }

    impl IMMNotificationClient_Impl for DefaultOutputNotifications {
        fn OnDefaultDeviceChanged(
            &self,
            flow: EDataFlow,
            role: ERole,
            _default_device_id: &PCWSTR,
        ) -> windows::core::Result<()> {
            if flow == eRender && role == eConsole {
                (self.on_change)();
            }
            Ok(())
        }

        fn OnDeviceStateChanged(
            &self,
            _device_id: &PCWSTR,
            _new_state: DEVICE_STATE,
        ) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnDeviceAdded(&self, _device_id: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnDeviceRemoved(&self, _device_id: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnPropertyValueChanged(
            &self,
            _device_id: &PCWSTR,
            _key: &PROPERTYKEY,
        ) -> windows::core::Result<()> {
            Ok(())
        }
    }

    /// This thread's use of COM, which creating the enumerator needs. The
    /// service thread joins the multithreaded apartment, since it runs no
    /// message loop; a thread already in a single-threaded apartment
    /// (`RPC_E_CHANGED_MODE`) serves as it is, and is not ours to uninitialize.
    struct ComApartment {
        initialized_here: bool,
    }

    impl ComApartment {
        fn enter() -> Result<Self, AudioError> {
            // SAFETY: no reserved pointer; balanced by `CoUninitialize` in `Drop`
            // when this call initialized COM.
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if result == RPC_E_CHANGED_MODE {
                return Ok(Self {
                    initialized_here: false,
                });
            }
            result.ok().map_err(|error| {
                AudioError::Unexpected(format!("CoInitializeEx failed: {error}"))
            })?;
            Ok(Self {
                initialized_here: true,
            })
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.initialized_here {
                // SAFETY: balances the successful `CoInitializeEx` in `enter`, on
                // the thread that made it (the listener never leaves the service
                // thread).
                unsafe { CoUninitialize() };
            }
        }
    }

    /// A registered notification client. Unregisters on drop, then releases the
    /// client, the enumerator and COM, in field order.
    pub(crate) struct DefaultDeviceListener {
        enumerator: IMMDeviceEnumerator,
        client: IMMNotificationClient,
        _com: ComApartment,
    }

    // The listener is opened on the service thread and dropped there; `Send` is
    // needed only because the device it belongs to is held behind
    // `AudioOutputDevice: Send`. The client's callback is `Send + Sync`, as
    // Windows calls it from a thread of its own.
    unsafe impl Send for DefaultDeviceListener {}

    impl DefaultDeviceListener {
        pub(super) fn register(callback: Callback) -> Result<Self, AudioError> {
            let com = ComApartment::enter()?;
            // SAFETY: COM is initialized on this thread (`com`).
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(
                    |error| {
                        AudioError::Unexpected(format!(
                            "creating the MMDevice enumerator failed: {error}"
                        ))
                    },
                )?;
            let client: IMMNotificationClient = DefaultOutputNotifications {
                on_change: callback,
            }
            .into();
            // SAFETY: `client` is held in the returned listener, so it outlives
            // the registration; `Drop` unregisters before releasing it, as the
            // registration (which takes no reference of its own) requires.
            unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }.map_err(
                |error| {
                    AudioError::Unexpected(format!(
                        "RegisterEndpointNotificationCallback failed: {error}"
                    ))
                },
            )?;
            Ok(Self {
                enumerator,
                client,
                _com: com,
            })
        }
    }

    impl Drop for DefaultDeviceListener {
        fn drop(&mut self) {
            // SAFETY: unregister the exact client registered in `register`.
            // Unregistering from inside a notification deadlocks (the interface
            // forbids it), so unregistering waits for a notification in progress
            // to return: once this returns, no callback is running and the
            // client can be released.
            if let Err(error) = unsafe {
                self.enumerator
                    .UnregisterEndpointNotificationCallback(&self.client)
            } {
                tracing::warn!(
                    "UnregisterEndpointNotificationCallback failed: {error}; \
                     the default-device listener may keep firing until process exit"
                );
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use windows::Win32::Media::Audio::{eCapture, eCommunications, eMultimedia};

        /// Windows reports a new default for every flow and role. Only the
        /// console render default is the device cpal opens, so only its change
        /// rebuilds the stream; the others would rebuild it for nothing, three
        /// times over for one switch.
        #[test]
        fn only_the_console_render_default_reaches_the_callback() {
            let changes = Arc::new(AtomicUsize::new(0));
            let counted = changes.clone();
            let notifications = DefaultOutputNotifications {
                on_change: Box::new(move || {
                    counted.fetch_add(1, Ordering::Relaxed);
                }),
            };
            let id = PCWSTR::null();

            for (flow, role) in [
                (eCapture, eConsole),
                (eCapture, eMultimedia),
                (eRender, eMultimedia),
                (eRender, eCommunications),
            ] {
                notifications
                    .OnDefaultDeviceChanged(flow, role, &id)
                    .unwrap();
            }
            assert_eq!(changes.load(Ordering::Relaxed), 0);

            notifications
                .OnDefaultDeviceChanged(eRender, eConsole, &id)
                .unwrap();
            assert_eq!(changes.load(Ordering::Relaxed), 1);
        }
    }
}
