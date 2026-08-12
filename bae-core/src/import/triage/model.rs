use super::*;

/// The sidebar's three lifecycle tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriageTab {
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
    /// import says it is. The percentage rides on
    /// [`TriageRow::import_status`].
    Importing,
    Done,
    Skipped,
}

impl TriagePlacement {
    pub fn tab(&self) -> TriageTab {
        match self {
            Self::Ready | Self::NeedsYou { .. } | Self::Importing => TriageTab::Pending,
            Self::Done => TriageTab::Done,
            Self::Skipped => TriageTab::Skipped,
        }
    }
}

/// The Needs-you groups, in the order the sidebar stacks them. Declaration
/// order *is* display order — [`NeedsYouGroup::IN_ORDER`] hands it to a surface
/// so neither UI invents its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NeedsYouGroup {
    /// Several pressings matched.
    PickAPressing,
    /// The disc ID and the barcode point at different releases.
    SignalsDisagree,
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
    pub const IN_ORDER: [Self; 6] = [
        Self::PickAPressing,
        Self::SignalsDisagree,
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
            NeedsYou::SignalsConflict => Self::SignalsDisagree,
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
/// produced nothing storable. The third is not rare —
/// [`TerminalVerdict::try_from`] refuses any terminal state carrying a recorded
/// lookup failure, so every network blip lands there — and rendering it as
/// "working on it" would promise progress that nothing is making.
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
            | IdentifyState::Conflict { .. }
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
    /// independent in the type, and naming a signal for a provenance that
    /// claims none would be worse than admitting there isn't one.
    fn of(provenance: &ResultProvenance) -> Option<Self> {
        if provenance.by_disc_id {
            Some(Self::DiscId)
        } else if provenance.by_barcode {
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

impl MatchedRelease {
    /// The release a verdict leads with, or `None` when it named none.
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
    pub(super) fn of(verdict: &TerminalVerdict) -> Option<Self> {
        let TerminalVerdict::Found {
            matches,
            provenance,
            ..
        } = verdict
        else {
            return None;
        };
        let lead = matches.first()?;
        let settled = matches.len() == 1;
        let signal = provenance.first().and_then(MatchedSignal::of);
        Some(Self {
            release_id: lead.release_id.clone(),
            title: lead.title.clone(),
            artist: lead.artist.clone(),
            pressing: settled.then(|| MatchedPressing {
                year: lead.year,
                format: lead.format.clone(),
                track_count: source_track_count(lead),
            }),
            cover_thumbnail_url: lead.cover_art.as_ref().map(|c| c.thumbnail_url.clone()),
            evidence: MatchEvidence {
                source: lead.source,
                // Index-aligned with `matches`, so the lead's provenance is the
                // first one.
                signal,
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

/// What a candidate's stored pick settled it on: the choice itself, and — for
/// a release — how that release reads.
///
/// The two travel together because the row leads with the identity the
/// candidate is settled on, and a pick is that identity: a manual search
/// settles a folder on a release the verdict never named, and a row reading the
/// verdict alone would go on showing the folder name while the pane shows the
/// release.
#[derive(Debug, Clone)]
pub struct Picked {
    pub pick: crate::import::IdentityPick,
    /// The picked release as its own archived documents describe it. `None`
    /// when the folder is read as its own tags, and when nothing archived the
    /// documents behind a release pick — the row then leads with the folder
    /// name until the pane's own read fetches them.
    pub release: Option<MatchedRelease>,
}

fn source_track_count(result: &MetadataResult) -> Option<u32> {
    match result.source_tracks {
        Some(SourceTracks::Listed { count, .. }) => Some(count),
        Some(SourceTracks::Nothing) | None => None,
    }
}

/// One candidate's row.
#[derive(Debug, Clone)]
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
    /// The release the row leads with. `None` and the folder name is the title.
    pub matched: Option<MatchedRelease>,
    /// Whether this row takes a bulk-import checkbox — exactly the Ready rows,
    /// which is the whole point of the Ready rule. Carried rather than left to
    /// each UI so the rule is stated once.
    pub selectable: bool,
    pub import_status: Option<CandidateImportStatusSnapshot>,
    /// The identity the user already chose for this candidate, read back from
    /// the stored row — what lets selection reopen the pane answered instead
    /// of asking again. `None` while they have chosen nothing.
    pub picked: Option<crate::import::IdentityPick>,
    /// The same decision in the shape commit takes, for a bulk import: it has
    /// no pane to read a claim line off, and turning a pick into an identity
    /// claim is not something a list should be working out. `None` alongside
    /// `picked` — nothing decided is nothing to commit.
    pub claim: Option<IdentityChoice>,
}

#[derive(Debug, Clone)]
pub struct TriageGroup {
    pub key: FolderReleaseDecisionKey,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum TriageEntry {
    Candidate(TriageRow),
    Boundary(FolderReleaseBoundary),
    Invalid(InvalidCandidate),
}

impl TriageEntry {
    /// Stable identity for a projected sidebar entry. Variant prefixes keep a
    /// candidate and a boundary at the same folder from sharing view state;
    /// the boundary length prefix makes its two path components unambiguous.
    pub fn stable_key(&self) -> String {
        match self {
            Self::Candidate(row) => format!("candidate:{}", row.candidate_key),
            Self::Boundary(boundary) => format!(
                "boundary:{}:{}{}",
                boundary.key.watched_folder_path.len(),
                boundary.key.watched_folder_path,
                boundary.key.relative_folder_path
            ),
            Self::Invalid(candidate) => format!("invalid:{}", candidate.path.display()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TriageSection {
    pub tab: TriageTab,
    pub watched_folder_path: String,
    pub group: Option<TriageGroup>,
    pub entries: Vec<TriageEntry>,
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
    pub(super) fn bump(&mut self, tab: TriageTab) {
        match tab {
            TriageTab::Pending => self.pending += 1,
            TriageTab::Done => self.done += 1,
            TriageTab::Skipped => self.skipped += 1,
        }
    }
}

/// The whole sidebar, grouped into the hierarchy both UIs render.
#[derive(Debug, Clone)]
pub struct TriageQueue {
    /// Tab-specific, core-shaped hierarchy. A section without `group` contains
    /// linear rows directly below the watched root.
    pub sections: Vec<TriageSection>,
    /// `skipped` counts the Skipped rows **plus** `invalid`.
    pub counts: TriageTabCounts,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
}

/// What a candidate's stored row says, read back.
///
/// The verdict and its classification travel as one value because they are the
/// same fact read two ways — the row's release comes from the verdict and its
/// placement from the classification — and pairing them here is what stops a
/// caller matching a verdict against somebody else's classification.
#[derive(Debug, Clone)]
pub struct Answered {
    pub verdict: TerminalVerdict,
    pub classification: QueueClassification,
}

impl Answered {
    /// Classify a stored verdict. `library_statuses` is a **live** check of the
    /// verdict's own matches — see [`classify`], which is why nothing here is
    /// cached.
    pub fn new(
        verdict: TerminalVerdict,
        probed_total_duration_ms: u64,
        library_statuses: &[LibraryStatus],
    ) -> Self {
        let classification = classify(&verdict, probed_total_duration_ms, library_statuses);
        Self {
            verdict,
            classification,
        }
    }
}
