//! The time zone the device is in, which the app tells core so that a play's
//! day is the day the device's calendar showed.

use bae_util::task_failure::Bug;

/// The device's time zone, which the app hands core after it opens a library.
/// Core calls it on its playback thread when a play begins.
#[uniffi::export(callback_interface)]
pub trait BridgeLocalTimeZone: Send + Sync {
    /// Seconds east of UTC that the device's clock shows at `unix_millis`,
    /// daylight saving time included.
    fn utc_offset_seconds(&self, unix_millis: i64) -> i32;
}

/// Core's view of the device's [`BridgeLocalTimeZone`]. A throw from the host
/// is reported, and that play records no row.
pub(crate) struct HostZone {
    zone: Box<dyn BridgeLocalTimeZone>,
    diagnostics: bae_diagnostics::Diagnostics,
}

impl HostZone {
    pub(crate) fn new(
        zone: Box<dyn BridgeLocalTimeZone>,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self { zone, diagnostics }
    }
}

impl std::fmt::Debug for HostZone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostZone").finish_non_exhaustive()
    }
}

impl bae_playback::LocalZone for HostZone {
    fn offset_at(
        &self,
        at: time::UtcDateTime,
    ) -> Result<time::UtcOffset, bae_playback::LocalZoneError> {
        use bae_util::timestamp::StoredTimestamp as _;
        let seconds: Result<i32, Bug> = crate::host_calls::host_call(
            |bug| self.diagnostics.panicked(bug),
            "read the device's time zone",
            || self.zone.utc_offset_seconds(at.unix_timestamp_millis()),
        );
        let seconds = seconds.map_err(|bug| bae_playback::LocalZoneError(bug.to_string()))?;
        time::UtcOffset::from_whole_seconds(seconds).map_err(|error| {
            bae_playback::LocalZoneError(format!(
                "the device's time zone answered {seconds} seconds east of UTC: {error}"
            ))
        })
    }
}

#[cfg(test)]
#[path = "local_time_zone_tests.rs"]
mod tests;
