//! What a test writes of a release's preparation: the person's pane edits
//! and file decisions, drafts and links applied from a source, and stored
//! lookups and the verdicts they conclude.

use super::*;

impl Library {
    /// The pane's controls over the release at `display_path`.
    pub(crate) fn pane(
        &self,
        display_path: &str,
    ) -> crate::tables::import_state::tests::PaneControls {
        crate::tables::import_state::tests::PaneControls::of(
            &self.db,
            &self.candidate(display_path),
        )
    }

    /// The person typing `value` into the release's album field `field`.
    pub(crate) fn typed(
        &self,
        display_path: &str,
        field: crate::CandidateEditField,
        value: &str,
    ) -> Write {
        let pane = self.pane(display_path);
        let hash = self.hash(display_path);
        let value = value.to_string();
        Write::step(async move {
            pane.type_field(&hash, field, &value).await.unwrap();
        })
    }

    /// The person typing `title` into the release's album title.
    pub(crate) fn draft(&self, display_path: &str, title: &str) -> Write {
        self.typed(display_path, crate::CandidateEditField::AlbumTitle, title)
    }

    /// The person's file decisions about the release at `display_path`
    /// saved, which moves its files to their next revision, clears its
    /// lookup and keeps its draft.
    pub(crate) fn file_decision(&self, display_path: &str) -> Write {
        let preparations = self.preparations();
        let tables = self.tables();
        let candidate = self.candidate(display_path);
        Write::step(async move {
            let hash = candidate.files.content_hash();
            let read = as_read(&tables, &hash).await;
            let draft = tables
                .load_candidate_preparation(&hash)
                .await
                .unwrap()
                .and_then(crate::tables::CandidatePreparationRead::stored)
                .expect("the candidate has a stored preparation")
                .metadata
                .draft;
            preparations
                .store_file_decisions(
                    &read,
                    &candidate.key(),
                    &crate::folder_scanner::CandidateFileEdits::default(),
                    &[(candidate.key(), candidate.files.clone())],
                    draft,
                )
                .await
                .unwrap();
        })
    }

    /// The person crediting the release's album to `credits`, in order.
    pub(crate) fn credit(&self, display_path: &str, credits: Vec<crate::Credited>) -> Write {
        let pane = self.pane(display_path);
        let hash = self.hash(display_path);
        Write::step(async move {
            pane.set_album_artists(&hash, &credits).await.unwrap();
        })
    }

    /// The person crediting the release's album to `artist` alone.
    pub(crate) fn credited_to(&self, display_path: &str, artist: &str) -> Write {
        self.credit(
            display_path,
            vec![crate::Credited::plain(crate::tables::import_state::tests::credit_named(artist))],
        )
    }

    /// The release's draft and its source replaced, as applying a source
    /// replaces them.
    pub(crate) fn metadata(
        &self,
        display_path: &str,
        draft: crate::RawReleaseEdit,
        provenance: Option<crate::MetadataProvenance>,
    ) -> Write {
        let preparations = self.preparations();
        let hash = self.hash(display_path);
        let key = self.key(display_path);
        Write::step(async move {
            preparations
                .replace_metadata(&hash, &key, &draft, provenance.as_ref(), &[])
                .await
                .unwrap();
        })
    }

    /// A draft of the release at `display_path` that would import: titled as
    /// its folder, credited to one artist, its track titled.
    pub(crate) fn valid_draft(&self, display_path: &str) -> Write {
        let title = display_path.rsplit('/').next().unwrap_or(display_path);
        self.metadata(
            display_path,
            crate::tables::import_state::tests::metadata_draft(title, "Artist"),
            None,
        )
    }

    /// The release's draft replaced by a blank one read from `provenance`:
    /// its files' tags that say nothing, or no source with `None`, as
    /// clearing the draft leaves it.
    pub(crate) fn blank_answer(
        &self,
        display_path: &str,
        provenance: Option<crate::MetadataProvenance>,
    ) -> Write {
        let mut draft = crate::tables::import_state::tests::metadata_draft("", "");
        for track in &mut draft.tracks {
            track.title.clear();
        }
        self.metadata(display_path, draft, provenance)
    }

    /// The release at `display_path` linked to `link`, or to nothing.
    pub(crate) fn link(&self, display_path: &str, link: Option<crate::ReleaseLink>) -> Write {
        let preparations = self.preparations();
        let hash = self.hash(display_path);
        Write::step(async move {
            preparations.replace_link(&hash, link.as_ref(), &[]).await.unwrap();
        })
    }

    /// The release at `display_path` linked to an album, its pressing
    /// unknown.
    pub(crate) fn linked(&self, display_path: &str) -> Write {
        self.link(
            display_path,
            Some(crate::ReleaseLink::Album(
                bae_identify::AlbumLink::new([bae_catalogs::records::MetadataRef::new(
                    bae_catalogs::catalog::Catalog::MusicBrainz,
                    format!("group-{display_path}"),
                )])
                .expect("one album of one catalog"),
            )),
        )
    }

    /// The lookup of the release at `display_path` stored as concluding
    /// `verdict`.
    pub(crate) fn concluded(&self, display_path: &str, verdict: TerminalVerdict) -> Write {
        self.concluded_at(display_path, verdict, 0)
    }

    /// The lookup of the release at `display_path`, run over its files'
    /// decisions at `revision`, stored as concluding `verdict`.
    pub(crate) fn concluded_at(
        &self,
        display_path: &str,
        verdict: TerminalVerdict,
        revision: u64,
    ) -> Write {
        let preparations = self.preparations();
        let row = NewImportCandidateVerdict {
            content_hash: self.hash(display_path),
            file_edit_revision: revision,
            candidate_key: self.key(display_path),
            verdict,
            signals: crate::tables::import_state::tests::settled_signals(),
            pick: None,
            releases: Vec::new(),
        };
        Write::step(async move {
            assert!(preparations.store_verdict(&row).await.unwrap(), "the verdict lands");
        })
    }

    /// A stored verdict of `kind` over a folder of `track_count` tracks,
    /// finding nothing.
    pub(crate) fn verdict(
        &self,
        display_path: &str,
        kind: &str,
        track_count: Option<u32>,
    ) -> Write {
        self.concluded(display_path, verdict_of(kind, track_count))
    }

    /// A found verdict over eleven tracks, whose releases agreement kept make
    /// `pressings` pressings, each listing `source_tracks`.
    pub(crate) fn found(
        &self,
        display_path: &str,
        pressings: u32,
        source_tracks: (Option<&str>, Option<i64>),
    ) -> Write {
        self.concluded(
            display_path,
            found_verdict(
                11,
                (0..pressings).map(|pressing| (found_release(pressing, source_tracks), pressing)),
            ),
        )
    }

    /// The person keeping their own draft over what the stored verdict
    /// offered.
    pub(crate) fn kept_own_draft(&self, display_path: &str) -> Write {
        let preparations = self.preparations();
        let tables = self.tables();
        let hash = self.hash(display_path);
        Write::step(async move {
            let read = as_read(&tables, &hash).await;
            preparations.keep_own_draft(&read).await.unwrap();
        })
    }
}

/// A verdict of `kind` over a folder of `track_count` tracks, finding
/// nothing.
pub(crate) fn verdict_of(kind: &str, track_count: Option<u32>) -> TerminalVerdict {
    let tracks = || track_count.expect("this verdict counts the folder's tracks");
    match kind {
        "not_found" => TerminalVerdict::NotFoundAnywhere { ledger: None },
        "manual_only" => TerminalVerdict::ManualOnly { track_count: tracks(), ledger: None },
        "failed" => TerminalVerdict::Failed {
            failures: vec![bae_identify::IdentifyFailure::Catalog(
                bae_catalogs::search::SourceFailure {
                    source: bae_catalogs::catalog::Catalog::MusicBrainz,
                    failure: bae_catalogs::LookupFailure::Network,
                },
            )],
            findings: bae_identify::Findings::default(),
            track_count: tracks(),
            ledger: None,
        },
        "error" => TerminalVerdict::Error {
            failure: bae_catalogs::InternalFailure { detail: "bae broke".to_string() },
        },
        "found" => found_verdict(tracks(), std::iter::empty()),
        other => panic!("no verdict is called {other}"),
    }
}

/// A found verdict over a folder of `track_count` tracks, offering each of
/// `offered` in its pressing row.
pub(crate) fn found_verdict(
    track_count: u32,
    offered: impl IntoIterator<Item = (MetadataResult, u32)>,
) -> TerminalVerdict {
    let (matches, pressings) = offered.into_iter().unzip();
    TerminalVerdict::Found {
        findings: bae_identify::Findings { matches, pressings, ..Default::default() },
        track_count,
        ledger: None,
    }
}

/// What a found verdict found, to change.
pub(crate) fn findings_of(verdict: &mut TerminalVerdict) -> &mut bae_identify::Findings {
    match verdict {
        TerminalVerdict::Found { findings, .. } | TerminalVerdict::Failed { findings, .. } => {
            findings
        }
        other => panic!("{other:?} found nothing"),
    }
}

/// The release `mb-{n}` a lookup found, listing `source_tracks`: its kind
/// (`listed`, `unreadable`, `nothing`) and count.
pub(crate) fn found_release(n: u32, source_tracks: (Option<&str>, Option<i64>)) -> MetadataResult {
    let source_tracks = match source_tracks {
        (Some("listed"), Some(count)) => Some(SourceTracks::Listed { count: count as u32 }),
        (Some("unreadable"), None) => Some(SourceTracks::Unreadable),
        (Some("nothing"), None) => Some(SourceTracks::Nothing),
        (None, None) => None,
        other => panic!("no release lists its tracks as {other:?}"),
    };
    MetadataResult {
        source: LookupCatalog::MusicBrainz,
        release_id: format!("mb-{n}"),
        title: "Album".to_string(),
        artist: None,
        year: None,
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: None,
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
        source_tracks,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}
