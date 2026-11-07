//! The formats bae's streams set Mac output devices to, and the ones it puts
//! back.
//!
//! cpal sets the macOS output device to each stream's sample rate as it builds
//! the stream (and, where the device has a 32-bit float format at that rate, to
//! that format). bae shouldn't leave the person's Mac that way, so every cpal
//! stream holds a [`FormatLease`] on the device it plays on, taken before cpal
//! builds the stream. The device runs in the format of the newest lease bae
//! holds on it; when that lease ends the device goes back to the format of the
//! newest one left, and when the last ends, to the format it had before bae
//! changed it.
//!
//! A lease outlives its stream: each player keeps the last one
//! ([`PlayerFormat`]) until its playback ends. The next stream of the same
//! playback — the next track at another rate, a rebuild on the same device —
//! takes its own lease first, so the device switches straight from the last
//! stream's format to the new one's instead of going back in between. The
//! kept lease ends when playback stops (the player says so,
//! [`PlayerFormat::playback_ended`]), when the app quits (the playback service
//! drops its players before its thread exits), and when the next stream plays
//! on another device because the default changed.
//!
//! cpal 0.18 reads and sets the device's format only inside its stream build,
//! so the reading and setting here are CoreAudio's, through the crates cpal
//! itself links: [`CoreAudioDeviceFormats`].

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tracing::{error, warn};

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
        let mut devices = self.devices();
        let id = self.next_lease.fetch_add(1, Ordering::Relaxed);
        match devices.get_mut(&device) {
            Some(device_use) => device_use.leases.push((id, None)),
            None => {
                let before_bae = self.control.format(device)?;
                devices.insert(device, DeviceUse { before_bae, leases: vec![(id, None)] });
            }
        }
        Ok(FormatLease { formats: self.clone(), device, id })
    }

    /// The devices and their leases. Each change to them is one map or list
    /// operation, and the CoreAudio calls made while they are held read or
    /// set a device, not them, so a panic in one never leaves them partway
    /// through a change, and they stand.
    fn devices(&self) -> MutexGuard<'_, HashMap<C::Device, DeviceUse<C::Format>>> {
        self.devices.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record the format the device runs in now as the one lease `id`'s
    /// stream set.
    fn settle(&self, device: C::Device, id: u64) -> Result<(), String> {
        let format = self.control.format(device)?;
        let mut devices = self.devices();
        let lease = devices
            .get_mut(&device)
            .and_then(|device_use| device_use.leases.iter_mut().find(|(lease, _)| *lease == id))
            .ok_or_else(|| format!("lease {id} on device {device:?} is not recorded"))?;
        lease.1 = Some(format);
        Ok(())
    }

    /// Lease `id` ended: put the device in the format of its newest stream
    /// left, or back in the one it had before bae when none is. Runs from the
    /// lease's `Drop`, so a lease it finds unrecorded is logged, not raised.
    fn release(&self, device: C::Device, id: u64) {
        let mut devices = self.devices();
        let Some(device_use) = devices.get_mut(&device) else {
            error!(
                ?device,
                id, "a live lease's device is not recorded; its format is not put back"
            );
            return;
        };
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
                warn!(?device, "couldn't put the output device's format back: {error}");
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

/// The lease of a player's last stream, kept after the stream ends until the
/// player's playback does (see the module docs). One per player output.
pub(crate) struct PlayerFormat<C: DeviceFormatControl> {
    kept: Arc<KeptLease<C>>,
}

impl<C: DeviceFormatControl> PlayerFormat<C> {
    pub(crate) fn new() -> Self {
        Self { kept: Arc::new(KeptLease(Mutex::new(None))) }
    }

    /// The lease of the stream just built, which comes back here when the
    /// stream ends. The lease kept from the previous stream ends now: its
    /// device already runs in the new stream's format when it is the same one,
    /// and goes back to its own when it isn't.
    pub(crate) fn stream_started(&self, lease: FormatLease<C>) -> StreamFormat<C> {
        self.kept.release();
        StreamFormat { lease: Some(lease), kept: self.kept.clone() }
    }

    /// The player's playback ended: let the kept lease go, putting its device
    /// back.
    pub(crate) fn playback_ended(&self) {
        self.kept.release();
    }
}

/// A stream's lease, handed back to its player when the stream ends.
pub(crate) struct StreamFormat<C: DeviceFormatControl> {
    lease: Option<FormatLease<C>>,
    kept: Arc<KeptLease<C>>,
}

impl<C: DeviceFormatControl> Drop for StreamFormat<C> {
    fn drop(&mut self) {
        self.kept.replace_with(self.lease.take());
    }
}

/// A player's kept lease. Letting it go or replacing it is one step, so a
/// panic while it was held — in the CoreAudio set a lease's drop makes — leaves
/// it whole, and it stands.
struct KeptLease<C: DeviceFormatControl>(Mutex<Option<FormatLease<C>>>);

impl<C: DeviceFormatControl> KeptLease<C> {
    /// Let the kept lease go.
    fn release(&self) {
        self.replace_with(None);
    }

    /// Keep `lease` in place of the kept one, which goes.
    fn replace_with(&self, lease: Option<FormatLease<C>>) {
        drop(std::mem::replace(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner), lease));
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
#[path = "device_formats_tests.rs"]
mod tests;
