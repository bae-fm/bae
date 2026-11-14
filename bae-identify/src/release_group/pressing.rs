//! One physical pressing as a row: the records that name it, which of them
//! leads, and what picking the row claims.

use super::{Judged, Statements};
use crate::agreements::Agreements;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::{LookupRef, MetadataRef};
use bae_catalogs::search::MetadataResult;

/// The pressing a candidate is linked to: the row a person picked, or
/// identification picked unattended, under every catalog's record of it that
/// the pick claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressingLink {
    /// The catalog's release the pick names first.
    pub record: LookupRef,
    /// The other catalogs' releases the picked pressing paired with. Find
    /// online pairs a MusicBrainz release and a Discogs release into one
    /// pressing row when the evidence their records carry says they name one
    /// physical object; picking the row claims both. Each names a different
    /// source from the primary and from every other partner.
    ///
    /// This is what the person picked, not what one provider says about
    /// another — a cross-reference an editor linked stays inferred from the
    /// documents.
    pub partners: Vec<LookupRef>,
}

impl PressingLink {
    /// Every release the link claims, the primary first.
    pub fn claimed(&self) -> impl Iterator<Item = &LookupRef> {
        std::iter::once(&self.record).chain(&self.partners)
    }
}

/// One physical pressing, under every record that names it. A row is picked
/// whole: `releases[0]` is the release the draft is read from, and each
/// further entry is the same pressing as another record has it — another
/// catalog's, or the same catalog's second record of one object. The
/// pressing's constructor says which record that first one is.
#[derive(Debug, Clone, PartialEq)]
pub struct Pressing {
    pub releases: Vec<MetadataResult>,
    /// What the row lists where its records state a fact, as the grouping
    /// worked it out when the row was formed.
    statements: Statements,
}

impl Pressing {
    /// One pressing's records, ordered by what the folder says about each.
    ///
    /// Every record describes the same physical object, and none of them is
    /// the one the draft is read from by name. The record whose tracklist fits
    /// the folder's audio is, since the draft's rows are laid onto the audio
    /// out of that tracklist and the verdict picks the row only when its lead
    /// fits; then the record the candidate's own text agrees with most; among
    /// records it says as much about, the one that states a tracklist. Where
    /// they are indistinguishable on all of that the source name decides,
    /// MusicBrainz first; and between one catalog's two records of the object,
    /// the one another record of the pressing names as the same release stands
    /// for that catalog, over the one nothing names.
    pub(super) fn of(mut releases: Vec<MetadataResult>, judged: &Judgements) -> Self {
        let named: Vec<bool> = releases
            .iter()
            .map(|release| {
                let reference = MetadataRef::from(release.release());
                releases.iter().any(|other| other.links.contains(&reference))
            })
            .collect();
        let mut order: Vec<usize> = (0..releases.len()).collect();
        order.sort_by_key(|&at| {
            let release = &releases[at];
            (
                judged.tracklist_rank(release),
                std::cmp::Reverse(judged.of_release(release).count()),
                !states_tracklist(release),
                release.source.catalog().rank(),
                !named[at],
            )
        });
        let mut releases: Vec<Option<MetadataResult>> = releases.drain(..).map(Some).collect();
        let releases: Vec<MetadataResult> = order
            .into_iter()
            .map(|at| releases[at].take().expect("each record is placed once"))
            .collect();
        Self::over(releases)
    }

    /// The row of `releases`, in the order they are given: the first leads.
    pub fn over(releases: Vec<MetadataResult>) -> Self {
        assert!(!releases.is_empty(), "a pressing is built from at least one release");
        Self { statements: Statements::of(&releases), releases }
    }

    /// The row whose records, in the order they lead, and whose statements a
    /// store holds: the rows a list laid out are read as they were formed.
    pub fn stored(releases: Vec<MetadataResult>, statements: Statements) -> Self {
        assert!(!releases.is_empty(), "a pressing is built from at least one release");
        Self { releases, statements }
    }

    /// What the row lists where its records state a fact.
    pub fn statements(&self) -> &Statements {
        &self.statements
    }

    /// Every catalog listing this pressing, in the order surfaces list
    /// catalogs: the row's tags, whichever record the row is read from.
    pub fn sources(&self) -> Vec<LookupCatalog> {
        LookupCatalog::ALL
            .into_iter()
            .filter(|catalog| self.releases.iter().any(|release| release.source == *catalog))
            .collect()
    }

    /// The release a row picks when the person picks the row itself.
    pub fn lead(&self) -> &MetadataResult {
        self.releases.first().expect("a pressing is built from at least one release")
    }

    /// What picking this row claims, as release references: the primary — the
    /// document the draft is read from — and, for every other catalog that
    /// names the pressing, its first record of it as a partner.
    ///
    /// A row is one pressing however many records name it, so this is the
    /// whole of what picking it means. A claim names one record per catalog:
    /// where a catalog lists the object twice, the record the folder says
    /// most about stands for it and the other is the same object already
    /// claimed. Deciding it here rather than on each surface is what keeps
    /// macOS, Windows, Linux and the sweep picking the same thing.
    pub fn claims(&self) -> (LookupRef, Vec<LookupRef>) {
        let mut releases = self.releases.iter().map(MetadataResult::release);
        let primary = releases.next().expect("a pressing is built from at least one release");
        let mut partners: Vec<LookupRef> = Vec::new();
        for release in releases {
            let claimed = release.catalog == primary.catalog
                || partners.iter().any(|partner| partner.catalog == release.catalog);
            if !claimed {
                partners.push(release);
            }
        }
        (primary, partners)
    }

    /// What the candidate's own text agrees with about this row: every one of
    /// its records' agreements together.
    ///
    /// A row is one physical object however many records name it, and it is
    /// picked whole, so a catalog number only Discogs prints and a disc ID
    /// only MusicBrainz answers are both true of the row.
    pub fn agreements(&self, judged: &Judgements) -> Agreements {
        self.releases
            .iter()
            .fold(Agreements::NONE, |so_far, release| so_far.with(judged.of_release(release)))
    }

    /// What picking this row claims — the primary release and each other
    /// catalog's record of the same pressing — as the link a pick stores.
    pub fn pick(&self) -> PressingLink {
        let (primary, partners) = self.claims();
        PressingLink { record: primary, partners }
    }
}

/// What the candidate's own text agrees with about each release, by the key
/// that tells releases apart, and how many tracks the folder holds, which
/// each release's tracklist is read against.
///
/// Built once from the judged results, and read back after they have been
/// paired: a pressing row is only known once pairing has run, and what the row
/// agrees with is what its records agree with.
#[derive(Debug, Clone, Default)]
pub struct Judgements {
    agreements: std::collections::HashMap<(LookupCatalog, String), Agreements>,
    /// `None` where no folder is being identified: a typed search, one
    /// lookup's own answer.
    folder_track_count: Option<u32>,
}

impl Judgements {
    /// What was said about these results, ready to be asked release by
    /// release, against a folder of `folder_track_count` tracks.
    pub fn of(results: &[Judged], folder_track_count: Option<u32>) -> Self {
        Self {
            agreements: results
                .iter()
                .map(|(release, agreements)| {
                    ((release.source, release.release_id.clone()), *agreements)
                })
                .collect(),
            folder_track_count,
        }
    }

    /// What was said about one release. A release these were not built from
    /// was never judged, which is nothing agreeing.
    fn of_release(&self, release: &MetadataResult) -> Agreements {
        self.agreements
            .get(&(release.source, release.release_id.clone()))
            .copied()
            .unwrap_or(Agreements::NONE)
    }

    /// Where a release's tracklist puts it among its pressing's records as
    /// the one the draft is read from. Every record ranks alike where there
    /// is no folder to read it against.
    fn tracklist_rank(&self, release: &MetadataResult) -> u8 {
        self.folder_track_count.map_or(0, |count| {
            crate::TracklistFit::of(release.source_tracks.as_ref(), count).lead_rank()
        })
    }
}

/// Whether the source listed this release's own tracks. A record that answered
/// with a tracklist has rows to fill a draft with and lengths to check the
/// audio against; one nobody has asked yet, and one that answered with
/// nothing, have neither.
fn states_tracklist(release: &MetadataResult) -> bool {
    matches!(release.source_tracks, Some(bae_catalogs::search::SourceTracks::Listed { .. }))
}

#[cfg(test)]
#[path = "pressing_tests.rs"]
mod tests;
