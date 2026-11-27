//! The two settings of coven's upload queue that a read shows by themselves.

// Whether the person paused uploads, whether or not anything is queued.
sqlive_compose::read!(
    UploadsPaused { paused: bool },
    r#"
    SELECT pause.paused AS paused
    FROM coven_upload_pause AS pause
"#
);

// Whether files move only on unmetered networks.
sqlive_compose::read!(
    UnmeteredOnly { unmetered_only: bool },
    r#"
    SELECT network.unmetered_only AS unmetered_only
    FROM coven_network AS network
"#
);
