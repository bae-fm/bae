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
///
/// Read from the tables alone. What identification or an import is doing is
/// not part of this: a run or an import is true of a candidate wherever it
/// sits, and is the row's [`CandidateLiveState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriagePlacement {
    /// Pending with nothing to import and nothing to ask: no verdict, or a
    /// verdict with nothing to ask over a draft that would not import.
    Pending,
    /// Metadata is prepared for import. The commands a row offers also account
    /// for what is running for it, which can keep a bulk import off it for a
    /// while — see [`CandidateActionBasis::actions`].
    Ready,
    NeedsYou {
        reason: NeedsYou,
    },
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
            | Self::Failed => TriageTab::Pending,
            Self::Done => TriageTab::Done,
            Self::Skipped => TriageTab::Skipped,
        }
    }

    /// A candidate an import has finished or failed is past the point where
    /// skipping it means anything: the attempt is what decides it now. One an
    /// import is running for is too, which is the live state's to say — see
    /// [`CandidateActionBasis::actions`].
    pub fn skip_action(&self) -> Option<TriageSkipAction> {
        match self {
            Self::Pending | Self::Ready | Self::NeedsYou { .. } => Some(TriageSkipAction::Skip),
            Self::Skipped => Some(TriageSkipAction::Unskip),
            Self::Failed | Self::Done => None,
        }
    }
}

/// The absolute skip-state command a placement allows, or absent once an import
/// has settled the candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageSkipAction {
    Skip,
    Unskip,
}

/// What identification is doing for a candidate right now.
///
/// A runtime fact, true of the candidate wherever its placement puts it: a
/// Ready row being identified again holds one, and so does a Needs-you row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentificationStatus {
    /// The candidate has been admitted but its driver has not started.
    Queued,
    /// Signals are being gathered or a provider lookup is in flight.
    Running,
    /// The identify reducer has a terminal result and the sweep is committing
    /// its verdict, metadata, and prepared documents.
    Finalizing,
    /// The terminal result could not be committed. The result stays available
    /// to the candidate pane and the diagnostic says why the row stopped.
    FinalizationFailed { error: String },
}

/// Which lookup produced a match — the row's trailing evidence chip, and the
/// confidence cue the design leans on.
///
/// Named strongest first, because a lead can be claimed by more than one and
/// the chip names one: a disc ID identifies the pressing, a barcode only the
/// product, and a title only what the folder calls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedSignal {
    /// The disc's table of contents.
    DiscId,
    Barcode,
    /// The run searched the catalogs for the candidate's own album title,
    /// which is what it falls back on when no identifier named anything.
    TitleSearch,
}

impl MatchedSignal {
    /// `None` when no lookup claims the result — a release the person picked
    /// themselves — and the row then shows the provider alone.
    fn of(lead: &LeadMatch) -> Option<Self> {
        if lead.by_disc_id {
            Some(Self::DiscId)
        } else if lead.by_barcode {
            Some(Self::Barcode)
        } else if lead.by_search {
            Some(Self::TitleSearch)
        } else {
            None
        }
    }
}

/// Which provider answered, and what matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvidence {
    pub source: Catalog,
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
/// This is independent of the verdict's lead: applying file metadata or editing a
/// chosen release changes the draft without changing what identification once
/// matched. The list owns this projection so every row keeps showing the
/// applied values when its detail subscription closes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageMetadataSummary {
    pub album_title: String,
    pub album_artist_assignments: Vec<crate::import::ArtistAssignment>,
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
        })
    }

    /// The same summary from what the stored draft's columns say: its album
    /// title and artists, and whether it is blank.
    pub(crate) fn of_columns(
        album_title: String,
        album_artist_assignments: Vec<crate::import::ArtistAssignment>,
        blank: bool,
        provenance: Option<&crate::import::MetadataProvenance>,
    ) -> Option<Self> {
        if blank && provenance.is_none() {
            return None;
        }
        Some(Self {
            album_title,
            album_artist_assignments,
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
    /// With several pressings the row still leads with the first one's title,
    /// artist and cover. Those are not group-level truths — a release group
    /// spans remasters and reissues that differ in all three — they are the
    /// lead pressing's, standing in for the album until someone picks. What is
    /// *not* shown is `pressing`: year, format and track count are the question
    /// being asked, and answering it from the first candidate would be the app
    /// pre-empting the user.
    pub fn of_summary(summary: &VerdictSummary) -> Option<Self> {
        let lead = summary.lead.as_ref()?;
        let settled = summary.pressing_count == 1;
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
    pub fn of_pick(source: Catalog, detail: &ImportSearchReleaseDetail) -> Self {
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
        Some(SourceTracks::Listed { count }) => Some(*count),
        Some(SourceTracks::Nothing) | None => None,
    }
}

/// What a row's text column says about its release: nothing yet, a draft, or a
/// draft one or more sources' releases were read into.
///
/// One value rather than a flag beside a source list, so "identified with no
/// source" and "prefilled from a source" are both unrepresentable. Decided
/// here rather than in each surface: two UIs deriving it from a summary and a
/// provenance is two answers to one question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft read off the files' tags, or typed in.
    Prefilled,
    /// A draft read from a catalog's release, with every catalog that
    /// describes it.
    Identified {
        records: Vec<crate::import::ReleaseRecord>,
    },
}

impl TriageReading {
    /// How a row reads, from the draft it carries, where that draft came
    /// from, and the records the pick's stored releases describe it in.
    ///
    /// `Unidentified` is exactly a row with no summary: the draft is blank and
    /// no catalog has been applied, so the row has a folder and nothing else.
    /// An external release reads as identified in exactly `records`: the
    /// caller derives them from the documents the pick claims, so every
    /// surface that names the release's catalogs names the same ones.
    pub fn of(
        summary: Option<&TriageMetadataSummary>,
        provenance: Option<&MetadataProvenance>,
        records: Vec<crate::import::ReleaseRecord>,
    ) -> Self {
        if summary.is_none() {
            return Self::Unidentified;
        }
        match provenance {
            Some(MetadataProvenance::ExternalRelease { .. }) => Self::Identified { records },
            Some(MetadataProvenance::FileMetadata) | None => Self::Prefilled,
        }
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
    /// Whether this release is folders a grouping reads as one, which the row
    /// offers to read as releases of their own.
    pub separable: bool,
    pub actionable: bool,
    pub placement: TriagePlacement,
    /// The Ready check this row did not pass, stated beside its Import:
    /// [`crate::import::triage::ready_check`] of its placement.
    pub ready_check: Option<NeedsYou>,
    /// What the row's commands are decided from in the tables. The commands
    /// themselves depend on what is running for the candidate too, so they
    /// are its [`CandidateLiveState`], read with this.
    pub action_basis: CandidateActionBasis,
    /// The release the row leads with. `None` and the folder name is the title.
    pub matched: Option<MatchedRelease>,
    /// The applied editable draft, independent of selection and of the
    /// identification result the row originally matched.
    pub metadata_summary: Option<TriageMetadataSummary>,
    /// The effective cover the row renders: selection, matched artwork, or the
    /// folder's default image.
    pub cover_thumbnail: Option<crate::import::CoverImageSource>,
    /// Whether a bulk import can take this row when nothing is running for
    /// it: [`CandidateActionBasis::importable_at_rest`]. What is running is
    /// checked when the import runs.
    pub selectable: bool,
    /// What the last import of this candidate left in the tables: the release
    /// it became, or the error it failed with. An import running now is the
    /// row's [`CandidateLiveState`].
    pub import_status: Option<TriageImportStatus>,
    /// The metadata provenance already applied to this candidate. `None` while no
    /// source has been selected.
    pub metadata_provenance: Option<crate::import::MetadataProvenance>,
    /// How the row's text column reads, with every catalog the pick's stored
    /// releases describe the release in.
    pub reading: TriageReading,
}

impl TriageRow {
    /// Every piece of text the row shows, which is what the list's filter
    /// tests: the draft's album title and each credited artist's name, or —
    /// with no draft, the [`TriageReading::Unidentified`] row — the folder's
    /// name, which is the row's title then.
    pub(crate) fn shown_text(&self) -> Vec<&str> {
        match &self.metadata_summary {
            None => vec![self.folder_name.as_str()],
            Some(summary) => std::iter::once(summary.album_title.as_str())
                .chain(
                    summary
                        .album_artist_assignments
                        .iter()
                        .map(crate::import::ArtistAssignment::name),
                )
                .collect(),
        }
    }
}

/// A Done row: the candidate that became a library release, presented as that
/// release.
///
/// Its own shape rather than a [`TriageRow`] placed Done, because what a Done
/// row shows is the library's and nothing of the candidate's: once the bytes
/// are in the library, the person re-identifies, edits and re-covers the
/// release there, and a row reading the candidate's draft or pick would go on
/// saying what the candidate said before any of that.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    pub display_path: String,
    /// What the row's commands are decided from in the tables, handed back
    /// with its live-state subscription: an import that just wrote the release
    /// can still own the candidate for a moment.
    pub action_basis: CandidateActionBasis,
    pub release: ImportedReleaseSummary,
}

/// The library release a Done row became, as the library has it now. Its
/// words are an [`ImportedReleaseText`], spread into the fields the surfaces
/// draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    /// [`ImportedReleaseText::title`].
    pub title: String,
    /// [`ImportedReleaseText::artist`].
    pub artist: Option<String>,
    /// [`ImportedReleaseText::year`].
    pub year: Option<i32>,
    /// The release's own cover.
    pub cover: Option<crate::album_detail::ImageRef>,
    /// Every catalog's description of the release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<crate::import::ReleaseRecord>,
}

/// What a Done row states about its library release in words. One read
/// answers it for the rows a window shows and for every Done row the list's
/// filter tests, so a row is found by the text it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedReleaseText {
    /// The album's title as the library holds it. Empty for a release reseeded
    /// from tags that named none, which the person fills in the editor.
    pub title: String,
    /// The album's credited artists as they show after merges, joined, or
    /// `None` when it credits none.
    pub artist: Option<String>,
    pub year: Option<i32>,
}

impl ImportedReleaseText {
    /// Every piece of text the row shows — its title, and its artist beside
    /// its year on the line under it — which is what the list's filter tests.
    pub(crate) fn shown_text(&self) -> Vec<std::borrow::Cow<'_, str>> {
        std::iter::once(std::borrow::Cow::Borrowed(self.title.as_str()))
            .chain(self.artist.as_deref().map(std::borrow::Cow::Borrowed))
            .chain(
                self.year
                    .map(|year| std::borrow::Cow::Owned(year.to_string())),
            )
            .collect()
    }
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

/// The outcome a candidate's last import finished with, read off the release
/// row an import wrote or the failure row one left behind.
#[derive(Debug, Clone, PartialEq)]
pub enum TriageImportStatus {
    Complete { release: ImportedRelease },
    Error { error: String },
}
