//! What identified the release an import claims.
//!
//! Picking a release claims that pressing — there is no album-only claim to
//! record. What is left to state is the evidence: which signal turned the
//! release up, so the user can weigh their own claim against it. The header
//! draws it as a badge; this module is what decides it.

use crate::identify::IdentifyState;
use crate::import::search::MetadataResult;
use crate::import::MetadataRef;

/// What identified the release a claim points at.
///
/// A disc ID is a fingerprint of the physical disc's table of contents, so a
/// lookup that returns one release has named the disc in the room. A barcode is
/// printed on the product and reissues reuse it; a catalog number or a typed
/// search names an edition at best. The header states which of these turned
/// the release up so the user can weigh their own claim against it — that is
/// the whole of what this decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimEvidence {
    /// The disc's table of contents matched this release and no other.
    DiscIdAlone,
    /// The disc's table of contents matched, but `match_count` releases came
    /// back for it. That says which album, not which pressing.
    DiscIdShared { match_count: u32 },
    /// A barcode read off the packaging matched.
    Barcode,
    /// A catalog number, or a search the user typed, found it.
    Search,
}

/// What a candidate's identify state says about one release.
///
/// A release the state does not mention was found some other way — a manual
/// search, or a pick made before the pipeline settled — and `Search` is the
/// honest answer for it: nothing about the disc in the room was matched.
pub fn evidence_for(state: &IdentifyState, release_ref: &MetadataRef) -> ClaimEvidence {
    match state {
        IdentifyState::Found {
            matches,
            provenance,
            ..
        } => {
            // `provenance` is index-aligned with `matches` by construction
            // (`combine` builds them together), so a missing entry would be a
            // bug rather than a state to render; treat it as "not mentioned".
            let Some(entry) = matches
                .iter()
                .position(|m| names(m, release_ref))
                .and_then(|index| provenance.get(index))
            else {
                return ClaimEvidence::Search;
            };
            if entry.by_disc_id {
                disc_id_evidence(provenance.iter().filter(|p| p.by_disc_id).count())
            } else if entry.by_barcode {
                ClaimEvidence::Barcode
            } else {
                ClaimEvidence::Search
            }
        }
        // The signals disagreed and the user picked a side. Which side names
        // this release is what its evidence is; the disc ID wins when both do.
        IdentifyState::Conflict { context } => {
            if context
                .discid_results
                .iter()
                .any(|(result, _)| names(result, release_ref))
            {
                disc_id_evidence(context.discid_results.len())
            } else if context
                .barcode_results
                .iter()
                .any(|(result, _)| names(result, release_ref))
            {
                ClaimEvidence::Barcode
            } else {
                ClaimEvidence::Search
            }
        }
        // Nothing matched, or nothing ran yet — whatever the user picked, they
        // found it themselves.
        IdentifyState::Idle
        | IdentifyState::Triangulating { .. }
        | IdentifyState::NotFoundAnywhere { .. }
        | IdentifyState::ManualOnly { .. } => ClaimEvidence::Search,
    }
}

/// A disc-ID match, sharp when it stands alone and blunt when it doesn't. A
/// count of zero can't happen (the caller found the release in that very set)
/// but reads as "alone" rather than panicking over an unreachable case.
fn disc_id_evidence(match_count: usize) -> ClaimEvidence {
    if match_count <= 1 {
        ClaimEvidence::DiscIdAlone
    } else {
        ClaimEvidence::DiscIdShared {
            match_count: match_count as u32,
        }
    }
}

/// Whether a result is the release the ref names. Both halves matter: two
/// sources can hand out the same id string.
fn names(result: &MetadataResult, release_ref: &MetadataRef) -> bool {
    result.source == release_ref.source && result.release_id == release_ref.id
}

#[cfg(test)]
mod tests;
