use super::*;
use bae_util::audio_container::AudioContainer;
use bae_util::test_channels::HANG_GUARD;

fn credential() -> SubsonicCredential {
    SubsonicCredential { username: "castuser".to_string(), password: "castpassword".to_string() }
}

/// Whether a browse is live right now — the gate the `cast_enabled` setting
/// exists to hold shut.
fn browsing(controller: &CastController) -> bool {
    controller.with_discovery(|discovery| match discovery {
        RendererDiscovery::Reported(reported) => reported.is_browsing(),
        RendererDiscovery::Builtin(_) => {
            panic!("the controller tests run over the reported device source")
        }
    })
}

/// Ask `controller` to browse for the picker, and wait until the ask is
/// decided: browsing starts once the setting is read.
fn start_discovery(runtime: &tokio::runtime::Runtime, controller: &Arc<CastController>) {
    start_discovery_for(runtime, controller, BrowsingFor::Picker);
}

/// Ask `controller` to browse for `asker`, and wait until the ask is decided.
fn start_discovery_for(
    runtime: &tokio::runtime::Runtime,
    controller: &Arc<CastController>,
    asker: BrowsingFor,
) {
    let mut decided = controller.browsing_decided.subscribe();
    decided.mark_unchanged();
    controller.start_discovery(asker);
    runtime.block_on(decided.changed()).expect("the controller is alive");
}

/// Write the casting setting, and wait until the controller has applied it.
fn set_casting(
    runtime: &tokio::runtime::Runtime,
    controller: &CastController,
    services: &AppServices,
    enabled: bool,
) {
    let mut applied = controller.setting_applied.subscribe();
    runtime.block_on(services.set_cast_enabled(enabled)).unwrap();
    runtime
        .block_on(applied.wait_for(|applied| *applied == Some(enabled)))
        .expect("the setting watcher runs while the controller is alive");
}

/// A controller over a fresh library, on the runtime it returns, serving
/// casts on loopback. The `TempDir` owns that library's files, so it must
/// outlive the controller.
fn test_controller(
    discovery: RendererDiscovery,
) -> (tokio::runtime::Runtime, Arc<CastController>, AppServices, tempfile::TempDir) {
    let (runtime, services, tmp) = bae_app::test_services::runtime_with_services();
    let controller = CastController::start_serving_on(
        services.clone(),
        runtime.handle().clone(),
        discovery,
        ServingNetwork::Loopback,
        ids(),
        Diagnostics::noop(),
    );
    (runtime, controller, services, tmp)
}

/// A loopback port nothing listens on: bound to take a free port from the
/// OS, then released, so a connect to it is refused at once.
fn refusing_loopback_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind a loopback port")
        .local_addr()
        .expect("read the bound port")
        .port()
}

/// A Cast service a host browser reports at `127.0.0.1:port`, so a connect
/// to it stays on this machine.
fn reported_cast_service(instance: &str, id: &str, port: u16) -> ReportedRenderer {
    ReportedRenderer {
        service_type: RendererServiceType::GoogleCast,
        instance_name: instance.to_string(),
        addr: "127.0.0.1".to_string(),
        port,
        txt: [("id", id), ("fn", instance)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// The gate the setting exists for: with casting off, asking to browse
/// starts no browse and asking to cast is refused outright — the UI hiding
/// its Cast control is a consequence, not the mechanism. Turning the setting
/// on makes the same two calls do their work. The gate is the controller's,
/// the same over either device source, so this runs over the reported one,
/// which holds no socket.
#[test]
fn discovery_and_casting_follow_the_cast_setting() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());

    assert!(!runtime.block_on(services.preferences()).unwrap().cast_enabled, "casting is opt-in");
    start_discovery(&runtime, &controller);
    assert!(!browsing(&controller), "casting is off: nothing may browse");
    assert!(
        matches!(runtime.block_on(controller.cast_to("some-device")), Err(CastError::Disabled)),
        "casting is off: a session must be refused"
    );

    runtime.block_on(services.set_cast_enabled(true)).unwrap();
    start_discovery(&runtime, &controller);
    assert!(browsing(&controller), "casting is on: browsing may run");
    // No device was reported, so the request reaches the device lookup and
    // fails there rather than at the gate.
    assert!(matches!(
        runtime.block_on(controller.cast_to("some-device")),
        Err(CastError::DeviceNotFound)
    ));

    controller.stop_discovery(BrowsingFor::Picker);
}

/// The picker closing while the setting is still being read leaves nothing
/// browsing: the stop is not undone by the read that finishes after it.
#[test]
fn a_stop_before_the_setting_is_read_wins() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());
    runtime.block_on(services.set_cast_enabled(true)).unwrap();

    let mut decided = controller.browsing_decided.subscribe();
    decided.mark_unchanged();
    controller.start_discovery(BrowsingFor::Picker);
    controller.stop_discovery(BrowsingFor::Picker);
    runtime.block_on(decided.changed()).expect("the controller is alive");
    assert!(!browsing(&controller), "the picker closed: nothing browses");
}

/// Turning the setting off while browsing stops it, rather than leaving the
/// browse running until the picker happens to close. The controller's own
/// watcher does this, so writing the setting is all a caller has to do.
#[test]
fn turning_casting_off_stops_browsing() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());

    runtime.block_on(services.set_cast_enabled(true)).unwrap();
    start_discovery(&runtime, &controller);
    assert!(browsing(&controller));

    let mut applied = controller.setting_applied.subscribe();
    runtime
        .block_on(applied.wait_for(|applied| *applied == Some(true)))
        .expect("the setting watcher runs while the controller is alive");
    runtime.block_on(services.set_cast_enabled(false)).unwrap();
    runtime
        .block_on(applied.wait_for(|applied| *applied == Some(false)))
        .expect("the setting watcher runs while the controller is alive");
    assert!(!browsing(&controller), "casting turned off: the watcher stopped browsing");
}

/// The Casting settings page browses while it is open, the same as the
/// picker, and either closing leaves the other's browse running: it ends with
/// the last of them.
#[test]
fn browsing_runs_while_the_settings_page_or_the_picker_is_open() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());
    set_casting(&runtime, &controller, &services, true);

    start_discovery_for(&runtime, &controller, BrowsingFor::SettingsPage);
    assert!(browsing(&controller), "the page is open: bae browses");

    start_discovery_for(&runtime, &controller, BrowsingFor::Picker);
    controller.stop_discovery(BrowsingFor::Picker);
    assert!(browsing(&controller), "the picker closed, the page is still open");

    start_discovery_for(&runtime, &controller, BrowsingFor::Picker);
    controller.stop_discovery(BrowsingFor::SettingsPage);
    assert!(browsing(&controller), "the page closed, the picker is still open");

    controller.stop_discovery(BrowsingFor::Picker);
    assert!(!browsing(&controller), "both closed: nothing browses");
}

/// A second place opening joins the browse running: it doesn't start a new
/// one, so what the first has found stays listed.
#[test]
fn a_second_place_opening_keeps_what_the_browse_found() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());
    set_casting(&runtime, &controller, &services, true);
    start_discovery_for(&runtime, &controller, BrowsingFor::SettingsPage);
    controller.renderer_found(reported_cast_service("Kitchen", "cast-1", 8009));

    start_discovery_for(&runtime, &controller, BrowsingFor::Picker);

    assert_eq!(controller.devices().len(), 1);
}

/// Casting turned off with the page still open stops the browse, and turning
/// it back on browses again, whichever order the page and the setting answer
/// in.
#[test]
fn casting_turned_off_and_on_follows_the_open_page() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());
    set_casting(&runtime, &controller, &services, true);
    start_discovery_for(&runtime, &controller, BrowsingFor::SettingsPage);

    set_casting(&runtime, &controller, &services, false);
    assert!(!browsing(&controller), "casting is off: nothing may browse");

    set_casting(&runtime, &controller, &services, true);
    assert!(browsing(&controller), "casting is on and the page is open");

    controller.stop_discovery(BrowsingFor::SettingsPage);
}

/// The reported path: a host that browses for bae pushes what its browser
/// resolved, and the picker's list follows — appearing on a report, gone on a
/// loss. Reports are refused while casting is off, exactly as browsing is, so
/// the setting is one gate rather than one per device source.
#[test]
fn reported_devices_follow_the_host_browser_and_the_cast_setting() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());

    let port = refusing_loopback_port();
    start_discovery(&runtime, &controller);
    controller.renderer_found(reported_cast_service("Kitchen", "cast-1", port));
    assert!(
        controller.devices().is_empty(),
        "casting is off: nothing browses and no report is taken"
    );

    runtime.block_on(services.set_cast_enabled(true)).unwrap();
    start_discovery(&runtime, &controller);
    controller.renderer_found(reported_cast_service("Kitchen", "cast-1", port));
    controller.renderer_found(reported_cast_service("Study", "cast-2", port));
    assert_eq!(
        controller.devices().iter().map(|device| device.name.as_str()).collect::<Vec<_>>(),
        vec!["Kitchen", "Study"],
        "a reported service is a device the picker can cast to"
    );

    controller.renderer_lost(RendererServiceType::GoogleCast, "Kitchen");
    assert_eq!(
        controller.devices().iter().map(|device| device.name.as_str()).collect::<Vec<_>>(),
        vec!["Study"],
        "a service the host's browser lost leaves the picker"
    );

    // Casting to a reported device reaches the real connect, which the
    // loopback port refuses, rather than stopping at the gate.
    let cast = runtime.block_on(controller.cast_to("cast-2"));
    assert!(matches!(cast, Err(CastError::Connect(_))), "{cast:?}");
}

/// The picker reads the devices from the library's `cast_device` table: a
/// reported service is listed there, by name, and one the host's browser
/// lost leaves it, each in a write of the whole list.
#[test]
fn the_picker_lists_the_devices_from_the_library() {
    let (runtime, controller, services, _tmp) = test_controller(RendererDiscovery::reported());
    let listed = services.subscribe_cast_devices();
    let until_listed = |names: &[&str]| {
        runtime.block_on(async {
            tokio::time::timeout(HANG_GUARD, async {
                loop {
                    let devices = listed.next().await.expect("the picker's list reads");
                    let shown: Vec<_> = devices.iter().map(|device| device.name.as_str()).collect();
                    if shown == names {
                        return devices;
                    }
                }
            })
            .await
            .unwrap_or_else(|_| panic!("the picker lists {names:?}"))
        })
    };
    until_listed(&[]);

    runtime.block_on(services.set_cast_enabled(true)).unwrap();
    start_discovery(&runtime, &controller);
    let port = refusing_loopback_port();
    controller.renderer_found(reported_cast_service("Study", "cast-2", port));
    controller.renderer_found(reported_cast_service("Kitchen", "cast-1", port));
    let devices = until_listed(&["Kitchen", "Study"]);
    assert_eq!(
        devices[0],
        bae_playback::renderer::CastDevice {
            id: "cast-1".to_string(),
            name: "Kitchen".to_string(),
            kind: bae_playback::renderer::RendererKind::Cast,
        }
    );

    controller.renderer_lost(RendererServiceType::GoogleCast, "Kitchen");
    until_listed(&["Study"]);
}

#[test]
fn ephemeral_server_start_does_not_reenter_the_owned_runtime() {
    let (runtime, controller, _services, _tmp) = test_controller(RendererDiscovery::reported());

    runtime.block_on(async {
        controller.ensure_server().await.expect("server starts from the controller runtime");
    });
    let base_url = controller.server.lock().unwrap().as_ref().unwrap().base_url.clone();
    assert!(
        base_url.starts_with("http://127.0.0.1:"),
        "the test's server stays on loopback: {base_url}"
    );
}

/// The id source a sink builds its sessions' ids from.
fn ids() -> coven::IdSourceRef {
    Arc::new(coven::SequentialIds::new())
}

/// A receiver advertising a setting bae can't read is refused before any
/// connection is attempted, with the setting named in the error.
#[test]
fn an_unreadable_airplay_advertisement_is_refused() {
    let unreadable =
        UnreadableSetting { key: "features".to_string(), raw: Some("garbage".to_string()) };
    let Err(error) = build_airplay_sink(
        &ids(),
        &Diagnostics::noop(),
        "10.0.0.7".parse().unwrap(),
        7000,
        &Err(unreadable.clone()),
    ) else {
        panic!("an unreadable advertisement builds no sink");
    };
    assert!(
        matches!(&error, CastError::AirPlayUnreadableSettings(setting) if *setting == unreadable)
    );
    assert!(error.to_string().contains("\"garbage\""));
}

/// An AirPlay 2 receiver that doesn't take the password-less transient
/// pairing — the only AirPlay 2 pairing bae does — is refused before any
/// connection is attempted, rather than tried with a pairing it rejects.
#[test]
fn an_airplay2_receiver_without_transient_pairing_is_refused() {
    // Unified media control (bit 38) without CoreUtils pairing (bit 48).
    let capabilities = AirPlayCapabilities::from_txt(
        RendererServiceType::AirPlay,
        &std::collections::HashMap::from([("features".to_string(), "0x0,0x40".to_string())]),
    );
    let Err(error) = build_airplay_sink(
        &ids(),
        &Diagnostics::noop(),
        "10.0.0.7".parse().unwrap(),
        7000,
        &capabilities,
    ) else {
        panic!("a receiver without transient pairing builds no sink");
    };
    assert!(matches!(error, CastError::AirPlayPairingUnsupported), "{error:?}");
    assert_eq!(error.to_string(), "this AirPlay receiver needs a pairing bae doesn't do");
}

/// A raw serve carries `format=raw`; an MP3 conversion carries
/// `format=mp3&maxBitRate=320`, a WAV one `format=wav`. Each names the
/// track as a `tr-` Subsonic id and carries the auth triplet.
#[test]
fn stream_url_encodes_format_and_track() {
    let provider = stream_url_provider("http://10.0.0.5:9000".to_string(), credential());

    let raw = provider("track-1", &RendererStreamFormat::Raw(AudioContainer::Flac)).unwrap();
    assert!(raw.starts_with("http://10.0.0.5:9000/rest/stream?id=tr-track-1&"));
    assert!(raw.contains("&format=raw"), "{raw}");
    assert!(raw.contains("u=castuser"));
    assert!(raw.contains("&t="));
    assert!(raw.contains("&s="));

    let transcoded = provider("track-2", &RendererStreamFormat::TranscodeMp3).unwrap();
    assert!(transcoded.contains("&format=mp3&maxBitRate=320"), "{transcoded}");

    let wav = provider("track-3", &RendererStreamFormat::TranscodeWav).unwrap();
    assert!(wav.contains("id=tr-track-3&"), "{wav}");
    assert!(wav.ends_with("&format=wav"), "{wav}");
}

/// A cover's URL names it by its Subsonic `coverArt` id, the release's album
/// id and the cover's version, so a replaced cover has a new URL.
#[test]
fn cover_url_names_the_cover_by_its_cover_art_id() {
    let provider = cover_url_provider("http://10.0.0.5:9000".to_string(), credential());
    let url = provider(&bae_records::ImageRef {
        id: "release-1".to_string(),
        version: "v2".to_string(),
        image_type: bae_records::LibraryImageType::Cover,
    });
    assert!(url.starts_with("http://10.0.0.5:9000/rest/getCoverArt?id=al-release-1~v2&"), "{url}");
    assert!(!url.contains("cover="), "{url}");
}

/// The token in a minted URL is `md5(password + salt)` over the URL's own
/// salt — the exact derivation bae-subsonic recomputes to authenticate the
/// request.
#[test]
fn minted_token_matches_the_credential() {
    let credential = credential();
    let url = stream_url_provider("http://host:1".to_string(), credential.clone())(
        "t",
        &RendererStreamFormat::Raw(AudioContainer::Flac),
    )
    .unwrap();

    let params: std::collections::HashMap<_, _> =
        url.split_once('?').unwrap().1.split('&').filter_map(|pair| pair.split_once('=')).collect();
    let salt = params["s"];
    let expected = bae_subsonic::token(&credential.password, salt);
    assert_eq!(params["t"], expected, "token must be md5(password + salt)");
}

/// Each minted URL carries a fresh salt, so the token differs between calls.
#[test]
fn each_url_uses_a_fresh_salt() {
    let provider = stream_url_provider("http://host:1".to_string(), credential());
    let a = provider("t", &RendererStreamFormat::Raw(AudioContainer::Flac)).unwrap();
    let b = provider("t", &RendererStreamFormat::Raw(AudioContainer::Flac)).unwrap();
    assert_ne!(a, b, "a fresh salt per call yields distinct URLs");
}

/// The ephemeral server binds a real port and authenticates the minted URLs
/// through bae-subsonic's own auth: a request with the minted token reaches
/// the handler (a missing track answers "not found", code 70 — not the
/// wrong-credentials code 40), a tampered token is rejected (code 40), and
/// stopping the server frees the port.
#[test]
fn ephemeral_server_authenticates_minted_urls_and_stops() {
    let (runtime, services, _tmp) = bae_app::test_services::runtime_with_services();

    runtime.block_on(async move {
        let server =
            EphemeralServer::start(services, ServingNetwork::Loopback, &Diagnostics::noop(), || {})
                .await
                .expect("the ephemeral server binds a real port");
        assert!(
            server.base_url.starts_with("http://127.0.0.1:"),
            "the test's server stays on loopback: {}",
            server.base_url
        );

        let base = server.base_url.clone();
        let valid = stream_url_provider(base.clone(), server.credential.clone())(
            "missing-track",
            &RendererStreamFormat::Raw(AudioContainer::Flac),
        )
        .unwrap();

        let body = reqwest::get(&valid)
            .await
            .expect("the server is reachable on its bound port")
            .text()
            .await
            .unwrap();
        assert!(
            !body.contains("code=\"40\""),
            "the minted token must authenticate (not a wrong-credentials error): {body}"
        );
        assert!(
            body.contains("code=\"70\""),
            "an authenticated request for a missing track answers not-found: {body}"
        );

        // Tamper with the token: auth must reject it.
        let tampered = valid
            .replace(&format!("&t={}", token_of(&valid)), "&t=00000000000000000000000000000000");
        let tampered_body = reqwest::get(&tampered).await.unwrap().text().await.unwrap();
        assert!(
            tampered_body.contains("code=\"40\""),
            "a tampered token must be rejected with wrong-credentials: {tampered_body}"
        );

        server.stop().await;

        // The port is released: a fresh request fails to connect.
        assert!(reqwest::get(&valid).await.is_err(), "stopping the session must release the port");
    });
}

/// Extract the `t` (token) value from a minted URL, for the tamper test.
fn token_of(url: &str) -> String {
    url.split('&')
        .find_map(|pair| pair.strip_prefix("t="))
        .expect("the URL carries a token")
        .to_string()
}

/// A controller no one holds is gone, and its tasks with it: none of them
/// holds it, so dropping the last handle ends it and drops the set that
/// aborts them.
#[test]
fn dropping_the_controller_ends_it_and_its_tasks() {
    let (runtime, controller, _services, _tmp) = test_controller(RendererDiscovery::reported());
    let weak = Arc::downgrade(&controller);
    drop(controller);
    runtime.block_on(async {
        tokio::time::timeout(HANG_GUARD, async {
            while weak.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("no task keeps the dropped controller alive");
    });
}
