//! Import progress events.
//!
//! Thin emitters that publish `ImportProgress` onto the broadcast bus as the
//! import advances through its phases.

use crate::import::types::{ImportPhase, ImportProgress};

use super::ImportService;

impl ImportService {
    /// Emit a running-phase progress event for the candidate row. `id` is the
    /// release id; `percent` fills the candidate's determinate bar when the
    /// phase can report a measured fraction.
    pub(super) fn emit_phase_progress(
        &self,
        run: super::ImportRun<'_>,
        id: &str,
        percent: Option<u8>,
        phase: ImportPhase,
    ) {
        Self::emit_phase_progress_on(&self.event_tx, run, id, percent, phase);
    }

    pub(super) fn emit_phase_progress_on(
        event_tx: &crate::import::handle::ImportEventBus,
        run: super::ImportRun<'_>,
        id: &str,
        percent: Option<u8>,
        phase: ImportPhase,
    ) {
        event_tx.send(crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: run.candidate_key.to_string(),
                progress: ImportProgress::Progress {
                    id: id.to_string(),
                    percent,
                    phase,
                    import_id: run.import_id.to_string(),
                },
            },
        );
    }
}
