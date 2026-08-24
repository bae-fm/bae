//! What identified the release an import claims, and which of the candidate's
//! files it was read off.
//!
//! Picking a release claims that pressing — there is no album-only claim to
//! record. What is left to state is the evidence: which signal turned the
//! release up. That statement belongs on the thing it was read from, not
//! beside the release: a barcode was OCR'd off one of the folder's images, a
//! disc ID was computed from one rip log or one cue sheet. So this names the
//! signal, its value, and the file — and a surface puts a chip on the tile or
//! the row that file already has.
//!
//! Evidence with no file behind it says nothing here. A release found through
//! a catalog number in the folder's own name, or through a search the user
//! typed, has no tile and no row to sit on, and gets no chip.

use crate::identify::IdentifyState;
use crate::import::search::MetadataResult;
use crate::import::MetadataRef;
use crate::signals::{DiscIdSignal, Signals};

/// A signal that can name the file it was read off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceSignal {
    /// A barcode read off one of the folder's images.
    Barcode,
    /// A disc ID computed from a rip log or a cue sheet.
    DiscId,
}

/// One piece of evidence that identification matched the release on, pinned to
/// the candidate file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvidence {
    pub signal: EvidenceSignal,
    /// The value itself — the barcode digits, the disc ID — as the chip's
    /// wording names it.
    pub value: String,
    /// The candidate-relative path of the file it was read off: the same id a
    /// gallery tile and a file row are keyed by.
    pub file_id: String,
}

/// Every piece of file-backed evidence the candidate's identify state has for
/// one release, read against the signals it settled on.
///
/// Empty when the state does not name the release (a pick the user made
/// themselves, or one made before the pipeline settled), and for evidence
/// whose origin is not a file.
pub fn file_evidence(
    state: &IdentifyState,
    release_ref: &MetadataRef,
    signals: &Signals,
) -> Vec<FileEvidence> {
    let IdentifyState::Found {
        matches,
        provenance,
        context,
        ..
    } = state
    else {
        return Vec::new();
    };
    // `provenance` is index-aligned with `matches` by construction (`combine`
    // builds them together), so a missing entry would be a bug rather than a
    // state to render; treat it as "not mentioned".
    let Some(entry) = matches
        .iter()
        .position(|result| names(result, release_ref))
        .and_then(|index| provenance.get(index))
    else {
        return Vec::new();
    };

    let mut evidence = Vec::new();
    if entry.by_disc_id {
        if let DiscIdSignal::Computed {
            disc_id,
            source_file: Some(file_id),
            ..
        } = &signals.disc_id
        {
            evidence.push(FileEvidence {
                signal: EvidenceSignal::DiscId,
                value: disc_id.clone(),
                file_id: file_id.clone(),
            });
        }
    }
    if entry.by_barcode {
        // Which barcode matched is the verdict's to say — several can be read
        // off one folder and only one found the release. The signal rows say
        // which image each was read off.
        if let Some(matched) = context.matched_barcode.as_deref() {
            if let Some(file_id) = signals
                .barcode
                .codes()
                .iter()
                .find(|code| code.value == matched)
                .and_then(|code| code.origin_path.clone())
            {
                evidence.push(FileEvidence {
                    signal: EvidenceSignal::Barcode,
                    value: matched.to_string(),
                    file_id,
                });
            }
        }
    }
    evidence
}

/// Whether a result is the release the ref names. Both halves matter: two
/// sources can hand out the same id string.
fn names(result: &MetadataResult, release_ref: &MetadataRef) -> bool {
    result.source == release_ref.source && result.release_id == release_ref.id
}

#[cfg(test)]
mod tests;
