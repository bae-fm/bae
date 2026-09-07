//! Projections over provider documents parsed once outside a database read.

use crate::discogs::client::DiscogsMaster;
use crate::discogs::DiscogsRelease;
use crate::import::cover_art::RemoteCover;
use crate::import::search::{ImportSearchReleaseDetail, SourceTracks};
use crate::import::{ImportError, MetadataSource, ParsedAlbum, ReleaseIdentity};
use crate::musicbrainz::MbReleaseResponse;

pub(super) enum ParsedDocuments {
    MusicBrainz {
        release: MbReleaseResponse,
        discogs: Option<DiscogsRelease>,
    },
    Discogs {
        release: DiscogsRelease,
        musicbrainz: Option<MbReleaseResponse>,
    },
}

/// One processing result owns the provider models used by all its projections.
pub struct ParsedReleasePayloads {
    pub(super) documents: ParsedDocuments,
    pub(super) master: Option<DiscogsMaster>,
}

impl ParsedReleasePayloads {
    /// The anchor's identity; a cross-reference contributes a separate identity
    /// when mapping the album, rather than replacing this claim.
    pub fn identity(&self) -> Result<ReleaseIdentity, ImportError> {
        match &self.documents {
            ParsedDocuments::MusicBrainz { release, .. } => {
                let group =
                    release
                        .release_group
                        .as_ref()
                        .ok_or_else(|| ImportError::SourceData {
                            metadata_source: MetadataSource::MusicBrainz,
                            detail: format!(
                                "MusicBrainz release {} names no release group",
                                release.id
                            ),
                        })?;
                Ok(ReleaseIdentity {
                    source: MetadataSource::MusicBrainz,
                    source_group_id: group.id.clone(),
                    source_release_id: release.id.clone(),
                })
            }
            ParsedDocuments::Discogs { release, .. } => {
                Ok(crate::import::discogs_mapper::discogs_identity(release))
            }
        }
    }

    pub fn source_tracks_for_audio(
        &self,
        audio_durations_ms: &[u64],
    ) -> Result<SourceTracks, ImportError> {
        Ok(match &self.documents {
            ParsedDocuments::MusicBrainz { release, .. } => {
                crate::import::search::mb_source_tracks(release)
            }
            ParsedDocuments::Discogs { release, .. } => {
                crate::import::search::discogs_source_tracks(release, Some(audio_durations_ms))
            }
        })
    }

    pub fn covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = Vec::new();
        match &self.documents {
            ParsedDocuments::MusicBrainz { release, discogs } => {
                covers.extend(crate::import::cover_art::musicbrainz_covers(release));
                if let Some(discogs) = discogs {
                    covers.extend(discogs.covers.iter().cloned());
                }
            }
            ParsedDocuments::Discogs {
                release,
                musicbrainz,
            } => {
                covers.extend(release.covers.iter().cloned());
                if let Some(musicbrainz) = musicbrainz {
                    covers.extend(crate::import::cover_art::musicbrainz_covers(musicbrainz));
                }
            }
        }
        if let Some(master) = &self.master {
            covers.extend(master.covers.iter().cloned());
        }
        let mut unique = Vec::new();
        for cover in covers {
            crate::import::cover_art::push_unique_cover(&mut unique, cover);
        }
        Ok(unique)
    }

    pub async fn gallery_covers(&self) -> Result<Vec<RemoteCover>, ImportError> {
        let mut covers = self.covers()?;
        covers.retain(|cover| cover.source != MetadataSource::MusicBrainz);
        let musicbrainz = match &self.documents {
            ParsedDocuments::MusicBrainz { release, .. } => Some(release),
            ParsedDocuments::Discogs { musicbrainz, .. } => musicbrainz.as_ref(),
        };
        if let Some(release) = musicbrainz {
            let mut gallery = crate::import::cover_art::musicbrainz_gallery(
                &release.id,
                release
                    .release_group
                    .as_ref()
                    .map(|group| group.id.as_str()),
            )
            .await?;
            match &self.documents {
                ParsedDocuments::MusicBrainz { .. } => {
                    gallery.extend(covers);
                    covers = gallery;
                }
                ParsedDocuments::Discogs { .. } => covers.extend(gallery),
            }
        }
        Ok(covers)
    }

    pub fn default_cover(&self) -> Result<Option<RemoteCover>, ImportError> {
        Ok(self.covers()?.into_iter().next())
    }

    pub fn detail_for_audio(
        &self,
        audio_durations_ms: &[u64],
    ) -> Result<ImportSearchReleaseDetail, ImportError> {
        let covers = self.covers()?;
        match &self.documents {
            ParsedDocuments::MusicBrainz { release, .. } => {
                crate::import::search::build_mb_detail(&release.id, release, covers)
            }
            ParsedDocuments::Discogs { release, .. } => {
                Ok(crate::import::search::build_discogs_detail(
                    release,
                    covers,
                    Some(audio_durations_ms),
                ))
            }
        }
    }

    fn master_year(&self, release: &DiscogsRelease) -> Option<u32> {
        match &self.master {
            Some(master) => master.year,
            None => release.year,
        }
    }

    pub fn parsed(
        &self,
        audio_durations_ms: &[u64],
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<ParsedAlbum, ImportError> {
        match &self.documents {
            ParsedDocuments::MusicBrainz { release, discogs } => {
                crate::import::musicbrainz_mapper::map_mb_response_to_db(
                    release,
                    None,
                    discogs.clone(),
                    clock,
                    ids,
                )
            }
            ParsedDocuments::Discogs {
                release,
                musicbrainz,
            } => crate::import::discogs_mapper::map_discogs_to_db(
                release,
                self.master_year(release),
                musicbrainz.as_ref(),
                Some(audio_durations_ms),
                clock,
                ids,
            ),
        }
    }
}
