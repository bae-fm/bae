//! Shared outbound HTTP client configuration.

use std::time::Duration;

/// User-agent sent on every bae-originated HTTP request (MusicBrainz, Discogs,
/// the Cover Art Archive, artist images). Some APIs reject requests without a
/// descriptive agent, so keep it identifying.
pub(crate) const USER_AGENT: &str = "bae/1.0 +https://github.com/bae-fm/bae";

/// Total per-request ceiling for a bounded JSON API call.
pub(crate) const API_TIMEOUT: Duration = Duration::from_secs(30);

/// A `reqwest` client builder pre-set with bae's user-agent. Callers add any
/// further settings (redirect policy, timeouts) and call `.build()` themselves
/// so they keep their own error handling.
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().user_agent(USER_AGENT)
}
