//! The formats bae's streams set Mac output devices to, and the ones it puts
//! back.
//!
//! cpal sets the macOS output device to each stream's sample rate as it builds
//! the stream (and, where the device has a 32-bit float format at that rate, to
//! that format). bae shouldn't leave the person's Mac that way, so every cpal
//! stream holds a [`FormatLease`] on the device it plays on, taken before cpal
//! builds the stream. The device runs in the format of the newest stream bae
//! has on it; when that stream ends the device goes back to the format of the
//! newest one left, and when the last ends, to the format it had before bae
//! changed it. A stream ends when playback stops, when the app quits (the
//! playback service drops its streams before its thread exits), and when the
//! default device changes, since the stream is then rebuilt on the new device.
//!
//! cpal 0.18 reads and sets the device's format only inside its stream build,
//! so the reading and setting here are CoreAudio's, through the crates cpal
//! itself links: [`CoreAudioDeviceFormats`].

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::warn;

/// Reads and sets an output device's format.
pub(crate) trait DeviceFormatControl: Send + Sync + 'static {
    type Device: Copy + Eq + Hash + std::fmt::Debug + Send;
    type Format: Copy + Send;

    fn format(&self, device: Self::Device) -> Result<Self::Format, String>;

    /// Set `device` to `format`, returning once the device runs in it.
    fn set_format(&self, device: Self::Device, format: Self::Format) -> Result<(), String>;
}

/// The devices bae's streams play on, each with the format it had before bae
/// changed it and the formats its streams set, oldest first.
pub(crate) struct DeviceFormats<C: DeviceFormatControl> {
    control: C,
    devices: Mutex<HashMap<C::Device, DeviceUse<C::Format>>>,
    next_lease: AtomicU64,
}

struct DeviceUse<F> {
    before_bae: F,
    /// Each live lease and the format its stream set the device to, once the
    /// stream is built.
    leases: Vec<(u64, Option<F>)>,
}

impl<C: DeviceFormatControl> DeviceFormats<C> {
    pub(crate) fn new(control: C) -> Arc<Self> {
        Arc::new(Self {
            control,
            devices: Mutex::new(HashMap::new()),
            next_lease: AtomicU64::new(0),
        })
    }

    /// Take a lease on `device` for a stream about to be built on it. The first
    /// lease on a device reads the format the device has now, the one it goes
    /// back to.
    pub(crate) fn lease(self: &Arc<Self>, device: C::Device) -> Result<FormatLease<C>, String> {
        let mut devices = self.devices.lock().unwrap();
        let id = self.next_lease.fetch_add(1, Ordering::Relaxed);
        match devices.get_mut(&device) {
            Some(device_use) => device_use.leases.push((id, None)),
            None => {
                let before_bae = self.control.format(device)?;
                devices.insert(
                    device,
                    DeviceUse {
                        before_bae,
                        leases: vec![(id, None)],
                    },
                );
            }
        }
        Ok(FormatLease {
            formats: self.clone(),
            device,
            id,
        })
    }

    /// Record the format the device runs in now as the one lease `id`'s
    /// stream set.
    fn settle(&self, device: C::Device, id: u64) -> Result<(), String> {
        let format = self.control.format(device)?;
        let mut devices = self.devices.lock().unwrap();
        let device_use = devices
            .get_mut(&device)
            .expect("a live lease's device is recorded");
        let lease = device_use
            .leases
            .iter_mut()
            .find(|(lease, _)| *lease == id)
            .expect("a live lease is recorded");
        lease.1 = Some(format);
        Ok(())
    }

    /// Lease `id` ended: put the device in the format of its newest stream
    /// left, or back in the one it had before bae when none is.
    fn release(&self, device: C::Device, id: u64) {
        let mut devices = self.devices.lock().unwrap();
        let device_use = devices
            .get_mut(&device)
            .expect("a live lease's device is recorded");
        let newest = device_use.leases.last().map(|(lease, _)| *lease) == Some(id);
        device_use.leases.retain(|(lease, _)| *lease != id);
        let restore = match device_use.leases.last() {
            None => {
                let before_bae = device_use.before_bae;
                devices.remove(&device);
                Some(before_bae)
            }
            // The stream that ended wasn't the newest, so the device already
            // runs in the newest one's format.
            Some(_) if !newest => None,
            Some((_, settled)) => *settled,
        };
        if let Some(format) = restore {
            if let Err(error) = self.control.set_format(device, format) {
                warn!(
                    ?device,
                    "couldn't put the output device's format back: {error}"
                );
            }
        }
    }
}

/// A stream's hold on the device it plays on. Dropping it, after the stream,
/// puts the device's format back as [`DeviceFormats`] describes.
pub(crate) struct FormatLease<C: DeviceFormatControl> {
    formats: Arc<DeviceFormats<C>>,
    device: C::Device,
    id: u64,
}

impl<C: DeviceFormatControl> FormatLease<C> {
    /// Record the format the stream's build set the device to.
    pub(crate) fn settle(&self) -> Result<(), String> {
        self.formats.settle(self.device, self.id)
    }
}

impl<C: DeviceFormatControl> Drop for FormatLease<C> {
    fn drop(&mut self) {
        self.formats.release(self.device, self.id);
    }
}

/// An output device's physical format, read and set through CoreAudio. The
/// physical format holds the rate (and the sample format) cpal changes, so
/// setting the one read before bae undoes either change.
#[cfg(target_os = "macos")]
pub(crate) struct CoreAudioDeviceFormats;

#[cfg(target_os = "macos")]
impl CoreAudioDeviceFormats {
    /// The system's default output device, the one cpal resolves for a stream
    /// on the default device.
    pub(crate) fn default_output_device() -> Option<objc2_core_audio::AudioDeviceID> {
        coreaudio::audio_unit::macos_helpers::get_default_device_id(false)
    }
}

#[cfg(target_os = "macos")]
impl DeviceFormatControl for CoreAudioDeviceFormats {
    type Device = objc2_core_audio::AudioDeviceID;
    type Format = objc2_core_audio_types::AudioStreamBasicDescription;

    fn format(&self, device: Self::Device) -> Result<Self::Format, String> {
        use objc2_core_audio::{
            kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
            kAudioStreamPropertyPhysicalFormat, AudioObjectGetPropertyData,
            AudioObjectPropertyAddress,
        };
        use std::ptr::NonNull;

        let address = AudioObjectPropertyAddress {
            mSelector: kAudioStreamPropertyPhysicalFormat,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut format = std::mem::MaybeUninit::<Self::Format>::zeroed();
        let mut size = std::mem::size_of::<Self::Format>() as u32;
        // SAFETY: the property is an `AudioStreamBasicDescription`, and `size`
        // says how much `format` holds; it is read only when the call succeeds.
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut format).cast(),
            )
        };
        if status != 0 {
            return Err(format!(
                "reading device {device}'s physical format failed with OSStatus {status}"
            ));
        }
        // SAFETY: the call succeeded, so it wrote the format.
        Ok(unsafe { format.assume_init() })
    }

    fn set_format(&self, device: Self::Device, format: Self::Format) -> Result<(), String> {
        // Waits until the device reports the new format, so a stream built
        // right after sees the device as it now is.
        coreaudio::audio_unit::macos_helpers::set_device_physical_stream_format(device, format)
            .map_err(|error| format!("setting device {device}'s physical format failed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device whose format is a sample rate, set by the test or by "cpal".
    #[derive(Default)]
    struct FakeDevices {
        rates: Mutex<HashMap<u32, u32>>,
        sets: Mutex<Vec<(u32, u32)>>,
    }

    impl DeviceFormatControl for Arc<FakeDevices> {
        type Device = u32;
        type Format = u32;

        fn format(&self, device: u32) -> Result<u32, String> {
            Ok(self.rates.lock().unwrap()[&device])
        }

        fn set_format(&self, device: u32, rate: u32) -> Result<(), String> {
            self.rates.lock().unwrap().insert(device, rate);
            self.sets.lock().unwrap().push((device, rate));
            Ok(())
        }
    }

    fn devices(rates: &[(u32, u32)]) -> Arc<FakeDevices> {
        let fake = Arc::new(FakeDevices::default());
        fake.rates.lock().unwrap().extend(rates.iter().copied());
        fake
    }

    /// What building a stream does to the device: cpal sets its rate.
    fn build_stream(fake: &Arc<FakeDevices>, device: u32, rate: u32) {
        fake.rates.lock().unwrap().insert(device, rate);
    }

    fn rate(fake: &Arc<FakeDevices>, device: u32) -> u32 {
        fake.rates.lock().unwrap()[&device]
    }

    /// Playback stopping ends the last stream, and the device goes back to
    /// the rate it had before bae. Sabotage: skip the restore on the last
    /// release, and the Mac stays at the track's rate.
    #[test]
    fn the_last_stream_ending_puts_the_device_back() {
        let fake = devices(&[(1, 48_000)]);
        let formats = DeviceFormats::new(fake.clone());

        let lease = formats.lease(1).unwrap();
        build_stream(&fake, 1, 44_100);
        lease.settle().unwrap();
        assert_eq!(rate(&fake, 1), 44_100);

        drop(lease);
        assert_eq!(rate(&fake, 1), 48_000);
    }

    /// Between two tracks at different rates the new stream is built on a
    /// device already put back; the rate restored is still the one from
    /// before bae, not the first track's.
    #[test]
    fn the_rate_from_before_bae_survives_a_change_of_track_rate() {
        let fake = devices(&[(1, 48_000)]);
        let formats = DeviceFormats::new(fake.clone());

        let first = formats.lease(1).unwrap();
        build_stream(&fake, 1, 44_100);
        first.settle().unwrap();
        drop(first);
        let second = formats.lease(1).unwrap();
        build_stream(&fake, 1, 96_000);
        second.settle().unwrap();
        drop(second);

        assert_eq!(rate(&fake, 1), 48_000);
    }

    /// A preview played over paused playback sets the device to the
    /// preview's rate; when the preview ends the device goes back to the
    /// paused track's rate, and only when that ends to the one from before bae.
    #[test]
    fn a_newer_stream_ending_puts_the_device_back_to_the_older_ones_rate() {
        let fake = devices(&[(1, 48_000)]);
        let formats = DeviceFormats::new(fake.clone());

        let main = formats.lease(1).unwrap();
        build_stream(&fake, 1, 44_100);
        main.settle().unwrap();
        let preview = formats.lease(1).unwrap();
        build_stream(&fake, 1, 96_000);
        preview.settle().unwrap();

        drop(preview);
        assert_eq!(rate(&fake, 1), 44_100);
        drop(main);
        assert_eq!(rate(&fake, 1), 48_000);
    }

    /// An older stream ending while a newer one plays leaves the device in
    /// the newer one's rate.
    #[test]
    fn an_older_stream_ending_leaves_the_newer_ones_rate() {
        let fake = devices(&[(1, 48_000)]);
        let formats = DeviceFormats::new(fake.clone());

        let main = formats.lease(1).unwrap();
        build_stream(&fake, 1, 44_100);
        main.settle().unwrap();
        let preview = formats.lease(1).unwrap();
        build_stream(&fake, 1, 96_000);
        preview.settle().unwrap();

        drop(main);
        assert_eq!(rate(&fake, 1), 96_000);
        assert!(fake.sets.lock().unwrap().is_empty(), "nothing was set");
        drop(preview);
        assert_eq!(rate(&fake, 1), 48_000);
    }

    /// When the default device changes, the stream on the old one is rebuilt
    /// on the new one: the old device goes back to its rate, and the new one's
    /// rate from before bae is the one it gets back later.
    #[test]
    fn a_change_of_device_puts_the_old_device_back() {
        let fake = devices(&[(1, 48_000), (2, 44_100)]);
        let formats = DeviceFormats::new(fake.clone());

        let on_old = formats.lease(1).unwrap();
        build_stream(&fake, 1, 96_000);
        on_old.settle().unwrap();
        drop(on_old);
        let on_new = formats.lease(2).unwrap();
        build_stream(&fake, 2, 96_000);
        on_new.settle().unwrap();

        assert_eq!(rate(&fake, 1), 48_000);
        drop(on_new);
        assert_eq!(rate(&fake, 2), 44_100);
    }

    /// A stream that failed to build still puts the device back: cpal may
    /// have set the rate before the build failed.
    #[test]
    fn a_failed_build_puts_the_device_back() {
        let fake = devices(&[(1, 48_000)]);
        let formats = DeviceFormats::new(fake.clone());

        let lease = formats.lease(1).unwrap();
        build_stream(&fake, 1, 44_100);
        drop(lease);

        assert_eq!(rate(&fake, 1), 48_000);
    }
}
