//! What an import claims to hold, and where its metadata came from.
//!
//! Every source-backed import records two separate facts. One is what the user
//! claims to physically hold — this specific release, or the album with the
//! pressing left open. The other is which release the metadata was read from,
//! and that is always one specific release whether or not it is the one being
//! claimed. They coincide in exactly one case; in every other the second fact
//! has to be stated, or the release the user just picked disappears from view.
//!
//! The claim is never asked as a question. It defaults from the evidence that
//! identified the candidate ([`ClaimEvidence`]) and moves only when the user
//! picks a different release, which re-derives the whole line. That derivation
//! happens here so neither desktop UI has to make it — they render
//! [`ClaimLine`] and decide nothing.

use crate::identify::IdentifyState;
use crate::import::search::{ImportSearchReleaseDetail, MetadataResult};
use crate::import::{IdentityChoice, MetadataRef};

/// What identified the release a claim points at.
///
/// Only a disc ID can pin a pressing: it is a fingerprint of the physical
/// disc's table of contents, so a lookup that returns one release has named the
/// disc in the room. A barcode is printed on the product and reissues reuse it;
/// a catalog number or a typed search names an edition at best. So exactly one
/// variant here claims the pressing — that is the rule the whole claim line
/// rests on, and [`ClaimEvidence::claims_pressing`] is the single place it is
/// written down.
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

impl ClaimEvidence {
    /// Whether this evidence is sharp enough to claim the pressing itself.
    pub fn claims_pressing(self) -> bool {
        matches!(self, Self::DiscIdAlone)
    }

    /// The claim this evidence defaults to for the release it identified.
    pub fn default_choice(self, release_ref: MetadataRef) -> IdentityChoice {
        if self.claims_pressing() {
            IdentityChoice::Exact { release_ref }
        } else {
            IdentityChoice::Approximate { release_ref }
        }
    }
}

/// A picked release reduced to the facts a claim line names.
///
/// Both paths that pick a release build one: the import confirm pane, from the
/// detail it prefetched ([`ClaimRelease::from_detail`]), and re-identify, from
/// the row the user clicked — it commits straight from the pick and never
/// prefetches, so the transport that carries that row builds this field by
/// field. A row states a track count only when the response it came from
/// carried a tracklist, which is why `track_count` is optional.
///
/// Neither the album title nor the artist is here — the header already states
/// those above the claim line, and repeating them would say nothing about
/// which pressing this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRelease {
    pub release_ref: MetadataRef,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub country: Option<String>,
    pub catalog_number: Option<String>,
    /// The source's own track count, where it has stated one. `None` on the
    /// search path, whose responses carry no tracklist at all.
    pub track_count: Option<u32>,
}

impl ClaimRelease {
    /// From the detail the import confirm pane prefetched, which always knows
    /// the source's track count because it fetched the tracklist.
    pub fn from_detail(detail: &ImportSearchReleaseDetail) -> Self {
        Self {
            release_ref: MetadataRef::new(detail.release_id.clone(), detail.source),
            year: detail.year,
            format: detail.format.clone(),
            country: detail.country.clone(),
            catalog_number: detail.catalog_number.clone(),
            track_count: Some(detail.track_count),
        }
    }
}

/// The release header's claim line: what the import claims you hold, why, and
/// which release the metadata was read from.
///
/// One `release` description serves both renderings, which is the point — a
/// pressing claim names the release inside its own sentence, an album claim
/// names it on the "metadata from" line, and neither hides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimLine {
    /// The claim this import will record, and what commit writes.
    pub choice: IdentityChoice,
    /// What identified the release, for the sentence's trailing clause.
    pub evidence: ClaimEvidence,
    /// The picked release by its pressing facts — format, year, country and
    /// catalog number, `·`-joined in that order. `None` when the source states
    /// none of them, which is a real thing a release stub does; the sentence
    /// then reads without a description rather than with an empty slot or an
    /// album title standing in for a pressing fact.
    pub release: Option<String>,
    /// The picked release's track count, where the source stated one. It rides
    /// the metadata-from line, because a differing track count is what tells a
    /// remaster from the edition it reissues when nothing else does.
    pub track_count: Option<u32>,
}

impl ClaimLine {
    /// Whether the header renders the second line naming the release the
    /// metadata came from. It appears exactly when the two facts differ — that
    /// is, whenever the claim is at the album level, since a pressing claim
    /// already names that release in its own sentence.
    pub fn shows_metadata_source(&self) -> bool {
        matches!(self.choice, IdentityChoice::Approximate { .. })
    }
}

/// The claim line for picking `release` while the candidate sits in `state`.
///
/// The identify state is the evidence: a release the disc-ID lookup returned
/// alone is claimed as a pressing, and everything else — including a release
/// the user found by searching, which no identify state mentions — is claimed
/// as the album.
pub fn claim_line(state: &IdentifyState, release: &ClaimRelease) -> ClaimLine {
    let evidence = evidence_for(state, &release.release_ref);
    ClaimLine {
        choice: evidence.default_choice(release.release_ref.clone()),
        evidence,
        release: describe_release(release),
        track_count: release.track_count,
    }
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

/// The picked release's pressing facts as one `·`-joined line, or `None` when
/// it states none of them.
fn describe_release(release: &ClaimRelease) -> Option<String> {
    let parts: Vec<String> = [
        release.format.clone(),
        release.year.map(|year| year.to_string()),
        release.country.clone(),
        release.catalog_number.clone(),
    ]
    .into_iter()
    .flatten()
    .map(|part| part.trim().to_string())
    .filter(|part| !part.is_empty())
    .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

#[cfg(test)]
mod tests;
