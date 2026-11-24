//! Image domain operations for [`LibraryManager`].

use super::*;
use bae_audio::tag_art::TagArtError;
use bae_pictures::{HeldOrigin, PictureOrigin};
use bae_records::CoverSource;
use std::io::{BufReader, Cursor, Read};

/// The bytes each read of a stored picture or audio file asks coven for: the
/// tags of most audio files, art and all, in one or two reads.
const STORED_READ_BYTES: usize = 256 * 1024;

/// A stored file read in ranges, holding its place of the open-files budget.
pub(super) type StoredOrigin =
    HeldOrigin<BufReader<bae_store::BlockingFileReader>, coven::FileLease>;

/// Why reading `what` through a blocking reader of a stored file failed: the
/// read's own failure when the stream failed, else the reader's.
fn read_error(what: &str, error: std::io::Error) -> LibraryError {
    let message = error.to_string();
    match error.into_inner().map(|source| source.downcast::<coven::FileReadError>()) {
        Some(Ok(failed)) => LibraryError::file_read(format!("read {what}"), *failed),
        _ => LibraryError::Storage(format!("read {what}: {message}")),
    }
}

/// The art the tags of the audio file `origin` holds, with `what` naming the
/// file in errors: a read that failed is the failure of the read, and a file
/// whose tags do not parse is a storage error.
fn tag_art_of(
    origin: impl Read + std::io::Seek,
    what: &str,
) -> Result<Option<(Vec<u8>, bae_util::content_type::ContentType)>, LibraryError> {
    bae_audio::tag_art::read_tag_art(origin).map_err(|error| match error {
        TagArtError::Read(error) => read_error(what, error),
        TagArtError::Parse(error) => {
            LibraryError::Storage(format!("read the tags of {what}: {error}"))
        }
    })
}

impl LibraryManager {
    /// Bytes of one gallery slot, dispatching the read on its [`GallerySource`]
    /// so no caller picks the byte source itself. `pixels` is the longer edge
    /// of the slot that draws it. A cover slot reads the copy that covers the
    /// size, and an image file's slot its preview; a size above the largest
    /// copy (the cover's 1536 pixels, the preview's 384) reads the picture
    /// itself: the cover's picture
    /// ([`read_cover_picture`](Self::read_cover_picture)), or the image file
    /// as the release holds it.
    /// The gallery carries a cover slot only when a cover exists, so a `Cover`
    /// with no bytes here is exceptional and surfaces rather than being masked.
    pub async fn read_gallery_bytes(
        &self,
        release_id: &str,
        source: &GallerySource,
        pixels: u32,
    ) -> Result<Vec<u8>, LibraryError> {
        match source {
            GallerySource::Cover(image) if pixels <= u32::from(CoverCopy::Large.step().edge()) => {
                self.read_image_bytes(image, Some(pixels)).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
                })
            }
            GallerySource::Cover(image) => {
                self.read_cover_picture(&image.id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover {} has no row", image.id))
                })
            }
            GallerySource::ReleaseFile { file_id }
                if pixels <= u32::from(NewReleasePreview::STEP.edge()) =>
            {
                self.read_release_preview(file_id).await
            }
            GallerySource::ReleaseFile { file_id } => {
                self.load_gallery_image(release_id, file_id).await
            }
        }
    }

    /// The picture the release's cover is made from, which the viewer opens:
    /// the image file it names, the art in the audio file it names, or the
    /// catalog image it keeps as its original. `None` when the release has no
    /// cover.
    pub async fn read_cover_picture(
        &self,
        release_id: &str,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let Some(mut origin) = self.open_cover_picture(release_id).await? else {
            return Ok(None);
        };
        let what = format!("the picture of {release_id}'s cover");
        let bytes = self
            .diagnostics
            .blocking("read a cover's picture", move || {
                let mut bytes = Vec::new();
                origin.read_to_end(&mut bytes).map(|_| bytes)
            })
            .await
            .map_err(|failed| LibraryError::Storage(failed.to_string()))?
            .map_err(|error| read_error(&what, error))?;
        Ok(Some(bytes))
    }

    /// The picture the release's cover is made from, opened to be read in
    /// ranges: the one place that decides which picture a cover opens. `None`
    /// when the release has no cover.
    pub(super) async fn open_cover_picture(
        &self,
        release_id: &str,
    ) -> Result<Option<Box<dyn PictureOrigin>>, LibraryError> {
        let Some(picture) = self.records.cover_picture(release_id).await? else {
            return Ok(None);
        };
        let what = format!("the picture of {release_id}'s cover");
        let origin: Box<dyn PictureOrigin> = match picture.source {
            CoverSource::Embedded { .. } => {
                Box::new(Cursor::new(self.read_tag_art(&what, &picture.file).await?))
            }
            CoverSource::ReleaseFile { .. } | CoverSource::Original => {
                Box::new(self.open_stored(&what, &picture.file).await?)
            }
        };
        Ok(Some(origin))
    }

    /// The coven file `file`, opened to be read in ranges by a thread that
    /// may block, holding one place of the open-files budget until it drops.
    pub(super) async fn open_stored(
        &self,
        what: &str,
        file: &coven::FileRef,
    ) -> Result<StoredOrigin, LibraryError> {
        let lease = bae_store::lease_one(&self.files).await;
        let stream = self
            .store
            .open_file_stream(file, &lease)
            .await
            .map_err(|error| LibraryError::file_read(format!("open {what}"), error))?;
        Ok(HeldOrigin::new(
            BufReader::with_capacity(
                STORED_READ_BYTES,
                stream.into_blocking_reader(self.runtime_handle.clone()),
            ),
            lease,
        ))
    }

    /// The art the tags of the audio file `file` hold, read from the stored
    /// file in ranges, so only its tags are read and none of its audio. A
    /// file whose tags hold no art is an error: a cover names it only because
    /// its tags held the art the cover was made from.
    async fn read_tag_art(
        &self,
        what: &str,
        file: &coven::FileRef,
    ) -> Result<Vec<u8>, LibraryError> {
        let origin = self.open_stored(what, file).await?;
        let what_owned = what.to_string();
        let art = self
            .diagnostics
            .blocking("read a cover's art from its audio file", move || {
                tag_art_of(origin, &what_owned)
            })
            .await
            .map_err(|failed| LibraryError::Storage(failed.to_string()))??;
        let (bytes, _) = art.ok_or_else(|| {
            LibraryError::Storage(format!("the tags of the audio file of {what} hold no art"))
        })?;
        Ok(bytes)
    }

    /// The preview of one of a release's image files, which the device that
    /// imported the release made from the original. A file with no preview is
    /// an error: every image file is written with its preview.
    async fn read_release_preview(&self, file_id: &str) -> Result<Vec<u8>, LibraryError> {
        let file = self.records.release_preview_file(file_id).await?.ok_or_else(|| {
            LibraryError::Storage(format!("release file {file_id} has no preview"))
        })?;
        self.read_image_file(&format!("preview of release file {file_id}"), &file).await
    }

    /// Bytes of one of a release's image files, read from the local copy when it
    /// exists here and otherwise downloaded from the release's storage (and
    /// decrypted). The `ReleaseFile` arm of [`read_gallery_bytes`](Self::read_gallery_bytes).
    async fn load_gallery_image(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let file = self.release_file(release_id, file_id).await?;
        self.read_release_file(&file).await
    }

    /// The file `file_id` of release `release_id`; the release has no such file
    /// when it is another release's or none.
    pub(super) async fn release_file(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<DbFile, LibraryError> {
        self.records.find_release_file(release_id, file_id).await?.ok_or_else(|| {
            LibraryError::FileMissing {
                release_id: release_id.to_string(),
                file_id: file_id.to_string(),
            }
        })
    }

    /// Change the cover art for an album's release. A `ReleaseImage` selection
    /// reads an image file already in the library by file id and makes the cover
    /// from that file; a `RemoteCover` one downloads the catalog's image from a
    /// URL and keeps it as the cover's original. Either way the cover's browse
    /// and large copies are made from the picture.
    #[cfg(desktop)]
    pub async fn change_cover(
        &self,
        release_id: &str,
        selection: CoverSelection,
    ) -> Result<(), LibraryError> {
        let (picture, bytes) = match selection {
            CoverSelection::ReleaseImage { file_id } => {
                let file = self.records.find_file_by_id(&file_id).await?.ok_or_else(|| {
                    LibraryError::FileMissing {
                        release_id: release_id.to_string(),
                        file_id: file_id.clone(),
                    }
                })?;

                // Read the chosen release image file through coven's locality-aware
                // read (the user's own file when Local, the cache/cloud when Remote).
                if file.release_id != release_id || !file.content_type.is_supported_cover() {
                    return Err(bae_import::ImportError::CoverArt {
                        detail: format!(
                            "{} is no image of release {release_id} that a cover can be made from",
                            file.original_filename
                        ),
                    }
                    .into());
                }
                let bytes = self.read_release_file(&file).await?;
                (NewCoverPicture::ReleaseFile { file_id }, bytes)
            }
            CoverSelection::RemoteCover { url } => {
                let bytes = self
                    .remote_images
                    .fetch_required(&url)
                    .await
                    .map_err(bae_import::ImportError::from)?
                    .bytes;
                (NewCoverPicture::Original { bytes: bytes.clone() }, bytes)
            }
        };

        let copies = bae_import::make_cover_copies(bytes, &self.diagnostics).await?;
        self.records
            .write_cover(NewCover { release_id: release_id.to_string(), picture, copies })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
