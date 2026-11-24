//! The names of a picture and of the size a request wants it at.

use std::path::PathBuf;

use crate::PictureStep;

/// A file on disk as a scan read it: a file replaced at the same path has
/// another size or time, and so is another picture.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified_at_ns: i64,
}

/// One picture bae can read, named by where it comes from and by the version
/// of what it comes from, so that a changed source is a new picture.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PictureSource {
    /// An image file in an import candidate's folder.
    FolderFile(SourceFile),
    /// Art embedded in the tags of an audio file of an import candidate:
    /// `file` is that audio file as the scan read it, and `source_file_id`
    /// names it among the candidate's files.
    EmbeddedArt { candidate_key: Vec<u8>, file: SourceFile, source_file_id: String },
}

/// What a request wants: a copy at a step, or the original's own bytes for a
/// viewer that decodes the picture at full size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PictureSize {
    Copy(PictureStep),
    Original,
}

impl PictureSize {
    /// The size a request for `pixels` wide on its longer side is made at:
    /// the step that covers it, or the original when it names none.
    pub fn covering(pixels: Option<u32>) -> Self {
        pixels.map_or(Self::Original, |pixels| Self::Copy(PictureStep::covering(pixels)))
    }
}
