//! Import progress events.
//!
//! Thin emitters that publish `ImportProgress` / `ImportLoudnessProgress`
//! onto the broadcast bus as the import advances through its phases.

use crate::import::handle::send_event;
use crate::import::types::{ImportPhase, ImportProgress};

use super::ImportService;

impl ImportService {
    pub(super) fn emit_started(&self, candidate_key: &str, release_id: &str, import_id: &str) {
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress: ImportProgress::Started {
                    id: release_id.to_string(),
                    import_id: import_id.to_string(),
                },
            },
        );
    }

    /// Emit a coarse running-phase progress event for the candidate row. `id` is
    /// the release id (or a track id, during the per-file referencing pass);
    /// `percent` fills the candidate's determinate bar for that phase.
    pub(super) fn emit_phase_progress(
        &self,
        candidate_key: &str,
        id: &str,
        percent: u8,
        phase: ImportPhase,
        import_id: &str,
    ) {
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress: ImportProgress::Progress {
                    id: id.to_string(),
                    percent,
                    phase,
                    import_id: import_id.to_string(),
                },
            },
        );
    }
}
