use super::*;

/// The sidebar's three lifecycle tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TriageTab {
    #[default]
    Pending,
    Done,
    Skipped,
}

/// Which tab a row belongs to, and whether its last import failed, read from
/// the tables alone; what is running for it is its [`CandidateLiveState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriagePlacement {
    /// Not imported and not skipped.
    Pending,
    /// The last import attempt failed; why is the row's [`TriageImportStatus`].
    Failed,
    Done,
    Skipped,
}

impl TriagePlacement {
    pub fn tab(&self) -> TriageTab {
        match self {
            Self::Pending | Self::Failed => TriageTab::Pending,
            Self::Done => TriageTab::Done,
            Self::Skipped => TriageTab::Skipped,
        }
    }

    /// The skip command the placement allows; none once an import finished or
    /// failed.
    pub fn skip_action(&self) -> Option<TriageSkipAction> {
        match self {
            Self::Pending => Some(TriageSkipAction::Skip),
            Self::Skipped => Some(TriageSkipAction::Unskip),
            Self::Failed | Self::Done => None,
        }
    }
}

/// The skip-state command a placement allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageSkipAction {
    Skip,
    Unskip,
}

/// What identification is doing for a candidate right now, whatever its
/// placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentificationStatus {
    /// The candidate has been admitted but its driver has not started.
    Queued,
    /// Signals are being gathered or a provider lookup is in flight.
    Running,
    /// The run has a result and is committing it.
    Finalizing,
    /// The result could not be committed; `failure` says why.
    FinalizationFailed { failure: crate::import::SaveFailure },
}

/// Which lookup produced a match, strongest first: the row's evidence chip
/// names the strongest one that claims it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedSignal {
    /// The disc's table of contents.
    DiscId,
    Barcode,
    /// The search by the audio's ISRCs.
    Isrc,
    /// A catalog search for the candidate's album title.
    TitleSearch,
}

impl MatchedSignal {
    /// `None` for a release the person picked themselves.
    fn of(lead: &LeadMatch) -> Option<Self> {
        if lead.by_disc_id {
            Some(Self::DiscId)
        } else if lead.by_barcode {
            Some(Self::Barcode)
        } else if lead.by_isrc {
            Some(Self::Isrc)
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

/// The facts that differ between editions of one album, present only once the
/// pressing is settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPressing {
    pub year: Option<i32>,
    /// Each carrier the source lists, with its count.
    pub media: Vec<crate::pressing::MediaCount>,
    /// The source's track count, when it listed one.
    pub track_count: Option<u32>,
}

/// The release identification matched for a row, kept on Skipped rows too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedRelease {
    /// The lead match's release id.
    pub release_id: String,
    /// The lead match's title, standing in for the album when several
    /// pressings matched.
    pub title: String,
    pub artist: Option<String>,
    pub pressing: Option<MatchedPressing>,
    /// The lead pressing's cover, in every size its catalog serves.
    pub cover: Option<crate::import::cover_art::RemoteImageSet>,
    pub evidence: MatchEvidence,
}

/// The candidate's stored draft as the row shows it, independent of what
/// identification matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageMetadataSummary {
    pub album_title: String,
    pub album_artist_assignments: Vec<crate::import::ArtistAssignment>,
}

impl TriageMetadataSummary {
    /// `None` for a blank draft of a candidate that is `answered` by nothing:
    /// no draft read from anywhere and no release link.
    pub(crate) fn of(draft: &crate::import::RawReleaseEdit, answered: bool) -> Option<Self> {
        if draft.is_blank() && !answered {
            return None;
        }
        Some(Self {
            album_title: draft.album_title.clone(),
            album_artist_assignments: draft.album_artist_assignments.clone(),
        })
    }

    /// The same summary from the stored draft's columns.
    pub(crate) fn of_columns(
        album_title: String,
        album_artist_assignments: Vec<crate::import::ArtistAssignment>,
        blank: bool,
        answered: bool,
    ) -> Option<Self> {
        if blank && !answered {
            return None;
        }
        Some(Self {
            album_title,
            album_artist_assignments,
        })
    }
}

impl MatchedRelease {
    /// The release a stored verdict leads with; its pressing only when the
    /// verdict found exactly one.
    pub fn of_summary(summary: &VerdictSummary) -> Option<Self> {
        let lead = summary.lead.as_ref()?;
        // A failed lookup may have named other pressings.
        let settled = summary.kind == crate::identify::VerdictKind::Found && summary.pressing_count == 1;
        Some(Self {
            release_id: lead.release_id.clone(),
            title: lead.title.clone(),
            artist: lead.artist.clone(),
            pressing: settled.then(|| MatchedPressing {
                year: lead.year,
                media: lead.media.clone(),
                track_count: source_track_count(&lead.source_tracks),
            }),
            cover: lead.cover.clone(),
            evidence: MatchEvidence {
                source: lead.source,
                signal: MatchedSignal::of(lead),
            },
        })
    }

    /// The release the person picked, as its documents describe it, its
    /// tracklist `source_tracks` as read against the folder's audio.
    pub fn of_pick(
        source: Catalog,
        detail: &ImportSearchReleaseDetail,
        source_tracks: SourceTracks,
    ) -> Self {
        Self {
            release_id: detail.release_id.clone(),
            title: detail.title.clone(),
            artist: detail.artist.clone(),
            pressing: Some(MatchedPressing {
                year: detail.year,
                media: detail.facts.media.clone(),
                track_count: source_track_count(&Some(source_tracks)),
            }),
            cover: detail.default_cover().map(|cover| cover.image.clone()),
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

/// What a row's text column says about its release: nothing yet, a draft, a
/// draft of a candidate linked to a catalog pressing, or one linked to an
/// album whose pressing is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft of a candidate linked to no release.
    Prefilled,
    /// A draft of a candidate linked to a catalog pressing, with every
    /// catalog that describes that pressing.
    Identified {
        records: Vec<crate::import::ReleaseRecord>,
    },
    /// A draft of a candidate linked to an album, with every catalog's record
    /// of the album: the row says its pressing is unknown.
    IdentifiedAlbum {
        records: Vec<crate::import::ReleaseRecord>,
    },
}

impl TriageReading {
    /// How a row reads from its draft, its release link, and the catalog
    /// records what it is linked to is described in.
    pub fn of(
        summary: Option<&TriageMetadataSummary>,
        link: Option<&ReleaseLink>,
        records: Vec<crate::import::ReleaseRecord>,
    ) -> Self {
        if summary.is_none() {
            return Self::Unidentified;
        }
        match link {
            Some(ReleaseLink::Pressing(_)) => Self::Identified { records },
            Some(ReleaseLink::Album(_)) => Self::IdentifiedAlbum { records },
            None => Self::Prefilled,
        }
    }

    /// Every catalog record the row names: none unless it is linked.
    pub fn records(self) -> Vec<crate::import::ReleaseRecord> {
        match self {
            Self::Identified { records } | Self::IdentifiedAlbum { records } => records,
            Self::Unidentified | Self::Prefilled => Vec::new(),
        }
    }
}

/// One candidate's row.
#[derive(Debug, Clone, PartialEq)]
pub struct TriageRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    /// The folder's name, which is the row's title while it has no draft.
    pub folder_name: String,
    /// The watched folder this candidate was scanned from
    /// (`WatchedFolder::path`).
    pub watched_folder_path: String,
    pub display_path: String,
    pub actionable: bool,
    pub placement: TriagePlacement,
    /// What the row's commands are decided from in the tables.
    pub action_basis: CandidateActionBasis,
    pub matched: Option<MatchedRelease>,
    pub metadata_summary: Option<TriageMetadataSummary>,
    /// The cover the row draws: the chosen one, the matched artwork, or the
    /// folder's default image.
    pub cover: Option<crate::import::CoverImageSource>,
    /// How the candidate's last import ended.
    pub import_status: Option<TriageImportStatus>,
    /// Where the candidate's draft was read from, once a source was applied.
    pub metadata_provenance: Option<crate::import::MetadataProvenance>,
    /// What the candidate is linked to in the catalogs.
    pub release_link: Option<ReleaseLink>,
    pub reading: TriageReading,
    /// Whether the person has selected the row.
    pub selected: bool,
}

impl TriageRow {
    /// The text the row shows, which the list's filter matches against.
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

/// A Done row: the library release the candidate became, as the library has it
/// now rather than as the candidate's draft said.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    pub display_path: String,
    /// What the row's commands are decided from; an import that just wrote the
    /// release can still own the candidate for a moment.
    pub action_basis: CandidateActionBasis,
    pub release: ImportedReleaseSummary,
    /// Whether the person has selected the row.
    pub selected: bool,
}

/// The library release a Done row became.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub cover: Option<crate::album_detail::ImageRef>,
    /// Every catalog's description of the release, in catalog order.
    pub records: Vec<crate::import::ReleaseRecord>,
}

/// The words a Done row shows for its library release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedReleaseText {
    /// Empty when the tags named no title.
    pub title: String,
    /// The album's credited artists, joined.
    pub artist: Option<String>,
    pub year: Option<i32>,
}

impl ImportedReleaseText {
    /// The text the row shows, which the list's filter matches against.
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
    /// releases, which the header offers to read as one.
    pub combinable: bool,
}

/// How many rows each tab holds, counted in the pass that places them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TriageTabCounts {
    pub pending: u32,
    pub done: u32,
    pub skipped: u32,
}

impl TriageTabCounts {
    /// How many entries `tab` holds.
    pub(crate) fn of(&self, tab: TriageTab) -> u32 {
        match tab {
            TriageTab::Pending => self.pending,
            TriageTab::Done => self.done,
            TriageTab::Skipped => self.skipped,
        }
    }

    pub(crate) fn bump(&mut self, tab: TriageTab) {
        match tab {
            TriageTab::Pending => self.pending += 1,
            TriageTab::Done => self.done += 1,
            TriageTab::Skipped => self.skipped += 1,
        }
    }
}

/// How a candidate's last import ended.
#[derive(Debug, Clone, PartialEq)]
pub enum TriageImportStatus {
    Complete { release: ImportedRelease },
    /// The last import failed, and why.
    Error {
        failure: crate::import::ImportFailureReason,
    },
    /// A release read from several folders that cannot be worked on, and why.
    Blocked { reason: crate::import::GroupingBlock },
}
