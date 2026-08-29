use super::*;

/// The sidebar's three lifecycle tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TriageTab {
    #[default]
    Pending,
    Done,
    Skipped,
}

/// Where a row sits within Pending, or which terminal tab it belongs to.
///
/// One field rather than a tab plus optional status fields, so an unresolved
/// row without a reason and an importable row with one are unrepresentable.
/// See `many-fields-none-together-means-a-missing-type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriagePlacement {
    /// Pending without a question or an automatic action in flight.
    Pending,
    /// Exactly one match, not in the library, counts and lengths agree — safe
    /// to import unattended.
    Ready,
    NeedsYou {
        /// The question this row batches under. Derived from `reason` at
        /// construction and never independently: group 3 collapses four
        /// classification variants into one question, so the two must not be
        /// able to disagree.
        group: NeedsYouGroup,
        /// The classification's own variant, kept whole so the row can name the
        /// disagreement precisely even when its group cannot.
        reason: NeedsYouReason,
    },
    /// An import claimed this candidate and has not finished. Its own variant
    /// rather than a Needs-you group: nothing is being asked of the user, and
    /// rather than Done, because the folder is not in the library until the
    /// import says it is. How far it has got is the candidate's runtime, read
    /// by the leaf that draws the bar.
    Importing,
    /// The last attempt to import this candidate failed and nothing has been
    /// attempted since. Pending, not Done: the folder is not in the library
    /// and the work is waiting on another attempt. Its own variant rather than
    /// a Needs-you group because nothing about the release is in question —
    /// the pick stands, the attempt did not. What went wrong is the row's
    /// [`TriageImportStatus::Error`], the same place the pane reads it.
    Failed,
    Done,
    Skipped,
}

impl TriagePlacement {
    pub fn tab(&self) -> TriageTab {
        match self {
            Self::Pending
            | Self::Ready
            | Self::NeedsYou { .. }
            | Self::Importing
            | Self::Failed => TriageTab::Pending,
            Self::Done => TriageTab::Done,
            Self::Skipped => TriageTab::Skipped,
        }
    }

    /// A candidate an import has claimed, finished or failed is past the point
    /// where skipping it means anything: the attempt is what decides it now.
    pub fn skip_action(&self) -> Option<TriageSkipAction> {
        match self {
            Self::Pending | Self::Ready | Self::NeedsYou { .. } => Some(TriageSkipAction::Skip),
            Self::Skipped => Some(TriageSkipAction::Unskip),
            Self::Importing | Self::Failed | Self::Done => None,
        }
    }
}

/// The absolute skip-state command available for a row, or absent after an
/// import has started. Carrying the command keeps every surface from deriving
/// lifecycle rules from placement independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageSkipAction {
    Skip,
    Unskip,
}

/// The Needs-you groups, in the order the sidebar stacks them. Declaration
/// order *is* display order — [`NeedsYouGroup::IN_ORDER`] hands it to a surface
/// so neither UI invents its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NeedsYouGroup {
    /// Several pressings matched.
    PickAPressing,
    /// The folder and the source disagree about what is here. Four
    /// classification variants share this group because they are one question
    /// to the user; the row's `reason` still names which.
    CountsOrLengthsDisagree,
    AlreadyInLibrary,
    /// Nothing matched, or there was nothing to look up.
    NoMatch,
    /// No verdict yet.
    StillIdentifying,
}

impl NeedsYouGroup {
    pub const IN_ORDER: [Self; 5] = [
        Self::PickAPressing,
        Self::CountsOrLengthsDisagree,
        Self::AlreadyInLibrary,
        Self::NoMatch,
        Self::StillIdentifying,
    ];

    /// The group a reason batches under. The one place the collapse happens.
    pub fn of(reason: &NeedsYouReason) -> Self {
        let needs_you = match reason {
            NeedsYouReason::StillIdentifying { .. } => return Self::StillIdentifying,
            NeedsYouReason::Disagreement(needs_you) => needs_you,
        };
        match needs_you {
            NeedsYou::SeveralMatches { .. } => Self::PickAPressing,
            NeedsYou::TrackCountDisagrees { .. }
            | NeedsYou::DurationsDisagree { .. }
            | NeedsYou::SourceLengthsUnknown
            | NeedsYou::LocalDurationUnknown => Self::CountsOrLengthsDisagree,
            NeedsYou::AlreadyInLibrary => Self::AlreadyInLibrary,
            NeedsYou::NoMatch | NeedsYou::NothingToLookUp => Self::NoMatch,
        }
    }
}

/// Why a row needs you.
///
/// [`NeedsYou`] rides along whole rather than being re-spelled here — it
/// already carries every operand a row states its disagreement with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedsYouReason {
    /// The stored verdict classified to this.
    Disagreement(NeedsYou),
    /// No stored verdict yet. Work in progress, not a decision anyone is being
    /// asked for — and `phase` says *which* kind of work in progress, because
    /// three unlike states share this group.
    StillIdentifying { phase: IdentifyPhase },
}

/// How far identification has got for a candidate with no stored verdict.
///
/// Without this the row cannot tell three different things apart, and the
/// design's dimmed group is supposed to show which: a candidate the sweep has
/// not reached, one being worked on right now, and one whose run *finished* but
/// produced nothing storable. The third is not rare — the conversion into
/// [`TerminalVerdict`](crate::identify::TerminalVerdict) refuses any terminal
/// state carrying a recorded lookup failure, so every network blip lands there
/// — and rendering it as "working on it" would promise progress that nothing is
/// making.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifyPhase {
    /// Nothing has run yet: the sweep has not reached this candidate.
    Queued,
    /// A run is in flight — signals are being gathered, or a lookup is out.
    Running,
    /// A run settled without an answer worth keeping: a lookup that never
    /// responded, so half the evidence is missing and nothing was stored. It is
    /// retried on a later pass; nobody is waiting on this one.
    ///
    /// A verdict that has just been accepted and is still being written reads
    /// this way for the moment between the two — which is the same shape as the
    /// answer arriving a moment later.
    NoAnswer,
}

impl IdentifyPhase {
    /// Read the phase off the candidate's live identify state. Only meaningful
    /// when no verdict is stored: with one stored, the classification is the
    /// answer and the phase says nothing.
    pub fn of(state: &IdentifyState) -> Self {
        match state {
            IdentifyState::Idle => Self::Queued,
            IdentifyState::Triangulating { .. } => Self::Running,
            IdentifyState::Found { .. }
            | IdentifyState::NotFoundAnywhere { .. }
            | IdentifyState::ManualOnly { .. } => Self::NoAnswer,
        }
    }
}

/// What is known about one candidate: the classification of its stored verdict,
/// or — with no verdict — how far identification has got.
///
/// One value rather than an `Option` beside a phase field, because the phase is
/// meaningless once a verdict exists and a caller should not be able to hand
/// over both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateAnswer {
    Classified(QueueClassification),
    Idle,
    Unanswered(IdentifyPhase),
}

/// Which signal produced a match — the row's trailing evidence chip, and the
/// confidence cue the design leans on.
///
/// Two variants because `combine` builds a `Found`'s matches out of exactly two
/// result sets. The design's third chip ("matched on text") has no producer
/// today; it arrives with the code that searches by text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedSignal {
    /// The disc's table of contents. Named ahead of the barcode when both
    /// lookups returned the release: a disc ID identifies the pressing, a
    /// barcode only the product.
    DiscId,
    Barcode,
}

impl MatchedSignal {
    /// `None` when neither lookup claims the result — the row then shows the
    /// provider alone. `combine` takes every match from one of the two result
    /// sets, so this does not arise from its output; the booleans are
    /// independent in the type, and naming a signal for a lead that claims
    /// none would be worse than admitting there isn't one.
    fn of(lead: &LeadMatch) -> Option<Self> {
        if lead.by_disc_id {
            Some(Self::DiscId)
        } else if lead.by_barcode {
            Some(Self::Barcode)
        } else {
            None
        }
    }
}

/// Which provider answered, and what matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvidence {
    pub source: MetadataSource,
    pub signal: Option<MatchedSignal>,
}

/// The pressing-level facts about a match — the ones that differ between the
/// editions of one album.
///
/// Present as a whole exactly when the pressing is settled, which is when the
/// verdict named one match. With several in play the row is *asking* which
/// pressing, so none of these is known, and absent-together is then a state
/// rather than a convention three separate `Option`s would leave a consumer to
/// honour. See `many-fields-none-together-means-a-missing-type`; `db::Pressing`
/// is the same shape for the same reason.
///
/// The fields stay optional inside it: a settled pressing may well state a year
/// and no format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPressing {
    pub year: Option<i32>,
    pub format: Option<String>,
    /// What the source says this release holds, once something asked. `None`
    /// when nobody has, or when the source answered and listed nothing.
    pub track_count: Option<u32>,
}

/// The release a row leads with. Absent as a whole when nothing matched — the
/// row then has the folder name as its title and no metadata line at all, so a
/// surface cannot render a half-populated match.
///
/// Populated for Done and Skipped rows too, deliberately: a candidate that has
/// been imported or set aside still shows what it was matched to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedRelease {
    /// The lead match's release id. A Ready row commits on exactly this
    /// release — a bulk import has no mapping pane to pick one in, so the row
    /// has to carry the id it will import against.
    pub release_id: String,
    /// The lead match's title. Titles vary between the editions of a release
    /// group, so with several matches this is one pressing's title standing in
    /// for the album — see `MatchedRelease::of`.
    pub title: String,
    /// The lead match's artist, with the same caveat as `title`.
    pub artist: Option<String>,
    /// The facts that are only known once the pressing is settled.
    pub pressing: Option<MatchedPressing>,
    /// [`crate::import::cover_art::RemoteCover::thumbnail_url`] of the lead
    /// match — a row renders a 40px cover, and the full-size URL is the mapping
    /// pane's business. Cover art is fetched per release id, so this is that one
    /// pressing's sleeve, not the group's.
    pub cover_thumbnail_url: Option<String>,
    pub evidence: MatchEvidence,
}

/// The candidate's stored editable metadata as one compact sidebar value.
///
/// This is independent of the verdict's lead: applying File Tags or editing a
/// chosen release changes the draft without changing what identification once
/// matched. The list owns this projection so every row keeps showing the
/// applied values when its detail subscription closes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageMetadataSummary {
    pub album_title: String,
    pub album_artist_assignments: Vec<crate::import::ArtistAssignment>,
    pub cover_thumbnail: Option<crate::import::CoverImageSource>,
}

impl TriageMetadataSummary {
    pub(crate) fn of(
        draft: &crate::import::RawReleaseEdit,
        provenance: Option<crate::import::MetadataProvenance>,
    ) -> Option<Self> {
        if draft.is_blank() && provenance.is_none() {
            return None;
        }
        Some(Self {
            album_title: draft.album_title.clone(),
            album_artist_assignments: draft.album_artist_assignments.clone(),
            cover_thumbnail: None,
        })
    }
}

impl MatchedRelease {
    /// The release a stored verdict leads with, read off the columns of its
    /// lead match row, or `None` when it named none.
    ///
    /// `Conflict`, `NotFoundAnywhere` and `ManualOnly` all lead with nothing:
    /// the first has results but no agreement on which is the match, and the
    /// other two have no results at all.
    ///
    /// With several matches the row still leads with the first one's title,
    /// artist and cover. Those are not group-level truths — a release group
    /// spans remasters and reissues that differ in all three — they are the
    /// lead pressing's, standing in for the album until someone picks. What is
    /// *not* shown is `pressing`: year, format and track count are the question
    /// being asked, and answering it from the first candidate would be the app
    /// pre-empting the user.
    pub fn of_summary(summary: &VerdictSummary) -> Option<Self> {
        let lead = summary.lead.as_ref()?;
        let settled = summary.match_count == 1;
        Some(Self {
            release_id: lead.release_id.clone(),
            title: lead.title.clone(),
            artist: lead.artist.clone(),
            pressing: settled.then(|| MatchedPressing {
                year: lead.year,
                format: lead.format.clone(),
                track_count: source_track_count(&lead.source_tracks),
            }),
            cover_thumbnail_url: lead.cover_thumbnail_url.clone(),
            evidence: MatchEvidence {
                source: lead.source,
                // Index-aligned with `matches`, so the lead's provenance is the
                // first one.
                signal: MatchedSignal::of(lead),
            },
        })
    }

    /// The release the user's own pick settled the candidate on, as its
    /// documents describe it.
    ///
    /// A pick names one release, so its pressing is settled by definition —
    /// there is no question left for the row to ask. No signal claims it
    /// either: a match somebody chose was not matched by a disc ID or a
    /// barcode, and the row shows the provider alone.
    pub fn of_pick(source: MetadataSource, detail: &ImportSearchReleaseDetail) -> Self {
        Self {
            release_id: detail.release_id.clone(),
            title: detail.title.clone(),
            artist: detail.artist.clone(),
            pressing: Some(MatchedPressing {
                year: detail.year,
                format: detail.format.clone(),
                track_count: Some(detail.track_count),
            }),
            cover_thumbnail_url: detail
                .default_cover()
                .map(|cover| cover.thumbnail_url.clone()),
            evidence: MatchEvidence {
                source,
                signal: None,
            },
        }
    }
}

fn source_track_count(source_tracks: &Option<SourceTracks>) -> Option<u32> {
    match source_tracks {
        Some(SourceTracks::Listed { count, .. }) => Some(*count),
        Some(SourceTracks::Nothing) | None => None,
    }
}

/// One candidate's row.
#[derive(Debug, Clone, PartialEq)]
pub struct TriageRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    /// The folder on disk: the mono subtitle, and the row's title when nothing
    /// matched.
    pub folder_name: String,
    /// The watched folder this candidate was scanned from — the sidebar's
    /// existing section key. Match it against `WatchedFolder::path`.
    pub watched_folder_path: String,
    pub display_path: String,
    pub resolved_boundaries: Vec<ResolvedFolderReleaseBoundary>,
    pub combine_ancestor_key: Option<FolderReleaseDecisionKey>,
    pub actionable: bool,
    pub placement: TriagePlacement,
    pub skip_action: Option<TriageSkipAction>,
    /// The release the row leads with. `None` and the folder name is the title.
    pub matched: Option<MatchedRelease>,
    /// The applied editable draft, independent of selection and of the
    /// identification result the row originally matched.
    pub metadata_summary: Option<TriageMetadataSummary>,
    /// Whether this row takes a bulk-import checkbox — exactly the Ready rows,
    /// which is the whole point of the Ready rule. Carried rather than left to
    /// each UI so the rule is stated once.
    pub selectable: bool,
    /// Where the candidate's import stands, without its progress: the row
    /// says *that* an import is running; how far along it is is the
    /// candidate's runtime, which ticks far more often than rows re-project.
    pub import_status: Option<TriageImportStatus>,
    /// The metadata provenance already applied to this candidate. `None` while no
    /// source has been selected.
    pub metadata_provenance: Option<crate::import::MetadataProvenance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriageGroup {
    pub key: FolderReleaseDecisionKey,
    pub name: String,
    /// Whether the rows under this header are this folder read as several
    /// releases, and so whether the header offers to read them as one. `false`
    /// where the header is only a path component the rows happen to share —
    /// there is nothing to combine and nothing was decided.
    ///
    /// The offer lives here and nowhere else: a row is a release, not a place
    /// to answer a question about the folder holding it.
    pub combinable: bool,
}

/// How many rows each tab holds. Computed in the same pass that places them, so
/// the number on a tab and the rows behind it cannot drift, and neither UI
/// counts an array length.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TriageTabCounts {
    pub pending: u32,
    pub done: u32,
    pub skipped: u32,
}

impl TriageTabCounts {
    pub(crate) fn bump(&mut self, tab: TriageTab) {
        match tab {
            TriageTab::Pending => self.pending += 1,
            TriageTab::Done => self.done += 1,
            TriageTab::Skipped => self.skipped += 1,
        }
    }
}

/// A candidate's import as the queue places it: claimed and running, or the
/// outcome it finished with, read off the release row an import wrote or the
/// failure row one left behind.
#[derive(Debug, Clone, PartialEq)]
pub enum TriageImportStatus {
    Importing,
    Complete { release: ImportedRelease },
    Error { error: String },
}
