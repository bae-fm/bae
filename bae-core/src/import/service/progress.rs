//! Import progress events.
//!
//! Thin emitters that publish `ImportProgress` onto the broadcast bus as the
//! import advances through its phases.

use crate::import::handle::send_event;
use crate::import::types::{ImportPhase, ImportProgress};

use super::ImportService;

impl ImportService {
    /// Emit a running-phase progress event for the candidate row. `id` is the
    /// release id; `percent` fills the candidate's determinate bar when the
    /// phase can report a measured fraction.
    pub(super) fn emit_phase_progress(
        &self,
        candidate_key: &str,
        id: &str,
        percent: Option<u8>,
        phase: ImportPhase,
        import_id: &str,
    ) {
        Self::emit_phase_progress_on(&self.event_tx, candidate_key, id, percent, phase, import_id);
    }

    pub(super) fn emit_phase_progress_on(
        event_tx: &tokio::sync::broadcast::Sender<crate::import::handle::ImportEvent>,
        candidate_key: &str,
        id: &str,
        percent: Option<u8>,
        phase: ImportPhase,
        import_id: &str,
    ) {
        send_event(
            event_tx,
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
