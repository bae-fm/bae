//! The covers the picker offers a candidate or a library release, and the
//! provider images it draws them from. A candidate's gallery is fetched and
//! returned; a library release's is fetched into the release's cover tables,
//! which `ImportTables::subscribe_cover_gallery` follows.

use super::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::rate_limiter::CallClass;
use bae_catalogs::remote_image::RemoteCover;

/// Whether an owner names an external release, and the artwork it offers.
/// An empty linked gallery is not an unidentified release.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteCoverGallery {
    Unlinked,
    Linked(Vec<RemoteCover>),
}

impl ImportServiceHandle {
    /// Bytes of provider art for a slot `pixels` wide on its longer side: the
    /// smallest copy the catalog serves that fills it, or the original when
    /// none does or `pixels` is `None`. The slot names its size and never a
    /// copy, so a large slot cannot be drawn from a thumbnail. Candidate
    /// preparation persists selected bytes independently; this session cache
    /// only avoids repeated transport within the process.
    ///
    /// `None` when the source serves no image at that address — an offered
    /// cover the archive turns out not to hold. The slot then renders as having
    /// no image, which is what it has, rather than as a failed load.
    pub async fn fetch_remote_image_bytes(
        &self,
        image: bae_catalogs::remote_image::RemoteImageSet,
        pixels: Option<u32>,
    ) -> Result<Option<bae_catalogs::remote_image::RemoteImage>, crate::ImportError> {
        Ok(self.services.remote_images.fetch(image.url_covering(pixels)).await?)
    }

    /// The bytes of the artwork embedded in one of the candidate's audio
    /// files, as its tag snapshot stored them: what a list row or the pane
    /// draws when its cover is [`crate::CoverImageSource::Embedded`], which
    /// names the artwork without carrying it. Refused when the snapshot holds
    /// none from `source_file_id`.
    pub async fn read_embedded_cover(
        &self,
        candidate_key: &crate::CandidateKey,
        source_file_id: &str,
    ) -> Result<Vec<u8>, crate::ImportError> {
        self.services
            .tables
            .load_embedded_cover_bytes(candidate_key, source_file_id)
            .await?
            .ok_or_else(|| {
                crate::CoverRefusal::NoTagArtwork { source_file_id: source_file_id.to_string() }
                    .into()
            })
    }

    /// Fetch the artwork the candidate's links offer, when its picker opens.
    pub async fn fetch_candidate_covers(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<RemoteCoverGallery, crate::ImportError> {
        self.candidate_covers(key).await
    }

    /// Fetch what the catalogs offer the library release's picker and store
    /// it in place of the last fetch's. A fetch that fails stores nothing, so
    /// the picker keeps showing the covers it had.
    pub async fn refresh_release_covers(&self, release_id: &str) -> Result<(), crate::ImportError> {
        let covers = match self.release_covers(release_id).await? {
            RemoteCoverGallery::Unlinked => None,
            RemoteCoverGallery::Linked(covers) => Some(covers),
        };
        self.services.tables.replace_release_covers(release_id, covers).await?;
        Ok(())
    }

    /// The live read of the covers a library release's picker shows; see
    /// `ImportTables::subscribe_cover_gallery`.
    pub fn subscribe_cover_gallery(
        &self,
        initial: Option<crate::CoverGalleryRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CoverGalleryRequest>,
        Option<crate::CoverGallery>,
        bae_store::DbError,
    > {
        self.services.tables.subscribe_cover_gallery(initial)
    }

    async fn candidate_covers(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<RemoteCoverGallery, crate::ImportError> {
        let candidate = self.require_release_candidate(key).await?;
        let state = self
            .services
            .tables
            .load_import_candidate_state(&candidate.files.content_hash())
            .await?;
        let link = match state.and_then(|state| state.release_link) {
            None => return Ok(RemoteCoverGallery::Unlinked),
            // An album's artwork is what each catalog shows for the album.
            Some(crate::ReleaseLink::Album(album)) => {
                return Ok(RemoteCoverGallery::Linked(
                    self.records_gallery(&album.records()).await?,
                ));
            }
            Some(crate::ReleaseLink::Pressing(pressing)) => pressing,
        };
        let mut claimed_releases = Vec::new();
        for claimed in link.claimed() {
            claimed_releases.push(
                self.services.tables.load_source_release(claimed).await?.ok_or_else(|| {
                    crate::ImportError::Internal {
                        detail: format!(
                            "{key} names {} release {} that is not stored",
                            claimed.catalog.as_str(),
                            claimed.key
                        ),
                    }
                })?,
            );
        }
        let (primary, partners) =
            claimed_releases.split_first().expect("a pick claims at least its primary");
        Ok(RemoteCoverGallery::Linked(
            self.services.catalogs.musicbrainz().pick_gallery_covers(primary, partners).await?,
        ))
    }

    async fn release_covers(
        &self,
        release_id: &str,
    ) -> Result<RemoteCoverGallery, crate::ImportError> {
        let records = self.services.records.get_release_records(release_id).await?;

        if records.is_empty() {
            return Ok(RemoteCoverGallery::Unlinked);
        }
        Ok(RemoteCoverGallery::Linked(self.records_gallery(&records).await?))
    }

    /// Every image the lookup catalogs among `records` show for what each
    /// record names, each once, in the records' order.
    async fn records_gallery(
        &self,
        records: &[bae_catalogs::records::ReleaseRecord],
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::ImportError> {
        let mut covers = Vec::new();
        for record in records {
            use bae_catalogs::records::ReleaseRecord;
            let Some(catalog) = record.catalog().lookup() else {
                continue;
            };
            let gallery = match (record, catalog) {
                (
                    ReleaseRecord::Pressing { release, album_key, .. },
                    LookupCatalog::MusicBrainz,
                ) => {
                    self.services
                        .catalogs
                        .musicbrainz()
                        .musicbrainz_gallery(&release.key, album_key.as_deref())
                        .await?
                }
                (ReleaseRecord::Pressing { release, .. }, LookupCatalog::Discogs) => {
                    self.services
                        .catalogs
                        .session()
                        .await?
                        .release_covers(&release.key, CallClass::AWAITED)
                        .await?
                }
                (ReleaseRecord::Album { album }, LookupCatalog::MusicBrainz) => {
                    self.services
                        .catalogs
                        .musicbrainz()
                        .musicbrainz_group_gallery(&album.key)
                        .await?
                }
                (ReleaseRecord::Album { album }, LookupCatalog::Discogs) => {
                    self.services
                        .catalogs
                        .session()
                        .await?
                        .master_covers(&album.key, CallClass::AWAITED)
                        .await?
                }
            };
            for cover in gallery {
                bae_catalogs::remote_image::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }
}

#[cfg(test)]
#[path = "covers_tests.rs"]
mod tests;
