//! The art an audio file's tags carry: the picture an import takes as a
//! release's cover, and the one the viewer opens again from the stored file.
//! Both read it here, so they pick the same picture.

use std::io::{Read, Seek};

use bae_util::content_type::ContentType;
use lofty::config::ParseOptions;
use lofty::file::TaggedFile;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::Tag;

/// Why an audio file's tags could not be read for their art.
#[derive(Debug, thiserror::Error)]
pub enum TagArtError {
    #[error("reading the file's first bytes failed: {0}")]
    Read(#[from] std::io::Error),
    #[error("its tags do not parse: {0}")]
    Parse(#[from] lofty::error::FileParseError),
}

/// The tag a file's other tags defer to: its format's own, else the first it
/// carries.
pub fn primary_tag(tagged: &TaggedFile) -> Option<&Tag> {
    tagged.primary_tag().or_else(|| tagged.first_tag())
}

/// The art `tag` carries in a format a cover can be made from: its front
/// cover, else its first such picture of any kind, with its content type.
pub fn tag_art(tag: &Tag) -> Option<(Vec<u8>, ContentType)> {
    let mut pictures = tag.pictures().iter().filter_map(|picture| {
        picture.mime_type().and_then(image_content_type).map(|content_type| (picture, content_type))
    });
    let (picture, content_type) = pictures
        .clone()
        .find(|(picture, _)| picture.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| pictures.next())?;
    Some((picture.data().to_vec(), content_type))
}

/// The content type of a picture a tag names `mime`, when a cover can be made
/// from it.
pub fn image_content_type(mime: &lofty::picture::MimeType) -> Option<ContentType> {
    let content_type = ContentType::from_mime(mime.as_str());
    content_type.is_supported_cover().then_some(content_type)
}

/// A reader that keeps the first failure of the reader under it, and hands
/// its caller an error of the same kind. A parser puts the error it reads
/// behind its own, so the failure's type survives only here.
struct FailureKept<R> {
    inner: R,
    failure: Option<std::io::Error>,
}

impl<R> FailureKept<R> {
    fn keep<T>(&mut self, result: std::io::Result<T>) -> std::io::Result<T> {
        result.map_err(|error| {
            let kind = error.kind();
            self.failure.get_or_insert(error);
            std::io::Error::new(kind, "the read of the file failed")
        })
    }
}

impl<R: Read> Read for FailureKept<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let result = self.inner.read(buffer);
        self.keep(result)
    }
}

impl<R: Seek> Seek for FailureKept<R> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let result = self.inner.seek(position);
        self.keep(result)
    }
}

/// The art the tags of the audio file `reader` holds, as [`tag_art`] picks
/// it from its [`primary_tag`]; `None` when it holds none. The read takes the
/// file's tags only: it skips the audio, so only the tag's bytes are read. A
/// read of the file that fails is [`TagArtError::Read`] with the reader's own
/// error, whether or not the parser met it first.
pub fn read_tag_art(
    reader: impl Read + Seek,
) -> Result<Option<(Vec<u8>, ContentType)>, TagArtError> {
    let mut reader = FailureKept { inner: reader, failure: None };
    let parsed = Probe::new(&mut reader)
        .options(ParseOptions::new().read_properties(false))
        .guess_file_type()
        .map_err(TagArtError::from)
        .and_then(|probe| probe.read().map_err(TagArtError::from));
    if let Some(failure) = reader.failure {
        return Err(TagArtError::Read(failure));
    }
    Ok(primary_tag(&parsed?).and_then(tag_art))
}

#[cfg(test)]
#[path = "tag_art_tests.rs"]
mod tests;
