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

/// One stream after another on a player — the next track at another
/// rate — switches the device straight from the first track's rate to the
/// second's; it goes back to its own only when playback ends. Sabotage:
/// release the lease when its stream ends, and the device goes back to
/// 48 kHz in between.
#[test]
fn consecutive_streams_of_a_playback_switch_the_device_directly() {
    let fake = devices(&[(1, 48_000)]);
    let formats = DeviceFormats::new(fake.clone());
    let player = PlayerFormat::new();

    let lease = formats.lease(1).unwrap();
    build_stream(&fake, 1, 44_100);
    lease.settle().unwrap();
    let first = player.stream_started(lease);
    drop(first);
    assert_eq!(rate(&fake, 1), 44_100, "the ended stream's rate stays");

    let lease = formats.lease(1).unwrap();
    build_stream(&fake, 1, 96_000);
    lease.settle().unwrap();
    let second = player.stream_started(lease);
    assert!(fake.sets.lock().unwrap().is_empty(), "nothing went back between the two tracks");
    drop(second);
    assert_eq!(rate(&fake, 1), 96_000);

    player.playback_ended();
    assert_eq!(rate(&fake, 1), 48_000);
    assert_eq!(*fake.sets.lock().unwrap(), [(1, 48_000)]);
}

/// The next stream on another device — the default changed — puts the
/// kept device back.
#[test]
fn the_next_stream_on_another_device_puts_the_kept_one_back() {
    let fake = devices(&[(1, 48_000), (2, 44_100)]);
    let formats = DeviceFormats::new(fake.clone());
    let player = PlayerFormat::new();

    let lease = formats.lease(1).unwrap();
    build_stream(&fake, 1, 96_000);
    lease.settle().unwrap();
    drop(player.stream_started(lease));

    let lease = formats.lease(2).unwrap();
    build_stream(&fake, 2, 96_000);
    lease.settle().unwrap();
    let on_new = player.stream_started(lease);

    assert_eq!(rate(&fake, 1), 48_000);
    drop(on_new);
    drop(player);
    assert_eq!(rate(&fake, 2), 44_100, "dropping the player ends its lease");
}
