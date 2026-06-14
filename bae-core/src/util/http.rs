//! Shared outbound HTTP client configuration.

/// User-agent sent on every bae-originated HTTP request (MusicBrainz, Discogs,
/// the Cover Art Archive, artist images). Some APIs reject requests without a
/// descriptive agent, so keep it identifying.
pub(crate) const USER_AGENT: &str = "bae/1.0 +https://github.com/bae-fm/bae";

/// A `reqwest` client builder pre-set with bae's user-agent. Callers add any
/// further settings (redirect policy, timeouts) and call `.build()` themselves
/// so they keep their own error handling.
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().user_agent(USER_AGENT)
}
