//! Reading one audio file's tags with lofty: what its tags state, what a
//! label or store wrote there, and its embedded artwork, with the legacy
//! ID3 text encodings decoded as the file wrote them.

use super::file_tag_snapshot::{DeliveryTags, FileTags};
use super::track_numbers::TrackNumbers;
use super::ImportError;
use bae_catalogs::StoreMarker;
use bae_util::content_type::ContentType;
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::id3::v2::{Frame, Id3v2Tag, Id3v2Version};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::TagType;
use lofty::TextEncoding;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub(crate) struct FileTagRead {
    pub tags: FileTags,
    pub delivery: DeliveryTags,
    pub embedded_cover: Option<(Vec<u8>, ContentType)>,
}

pub(crate) trait FileTagReader: Send + Sync {
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError>;
}

pub(crate) struct LoftyFileTagReader;

/// The tag a file's other tags defer to: its format's own, else the first it
/// carries.
fn primary_tag(tagged: &lofty::file::TaggedFile) -> Option<&lofty::tag::Tag> {
    tagged.primary_tag().or_else(|| tagged.first_tag())
}

impl FileTagReader for LoftyFileTagReader {
    /// Opens and parses `path` once: the concrete format's own reading, for
    /// the ID3v2 frames and MP4 atoms only it keeps, is the one the generic
    /// tag is taken from.
    fn read(&self, path: &Path) -> Result<FileTagRead, ImportError> {
        let file_type = FileType::from_path(path);
        let file = File::open(path).map_err(|error| ImportError::file_tags("open", path, error))?;
        let mut reader = BufReader::new(file);
        let parsed = parse(&mut reader, path, file_type)?;
        let tag = primary_tag(&parsed.tagged);
        let store = store_marker(parsed.itunes_purchase, tag);
        let Some(tag) = tag else {
            return Ok(FileTagRead {
                tags: FileTags::untagged(),
                delivery: DeliveryTags { store, ..DeliveryTags::default() },
                embedded_cover: None,
            });
        };
        let legacy = match tag.tag_type() {
            TagType::Id3v2 => match parsed.id3v2 {
                Id3v2Frames::Kept(Some(legacy)) => legacy,
                Id3v2Frames::Kept(None) => {
                    return Err(ImportError::FileTags {
                        detail: format!(
                            "{} was reported as ID3v2-tagged but its concrete tag was absent",
                            path.display()
                        ),
                    })
                }
                Id3v2Frames::Unkept => {
                    return Err(ImportError::FileTags {
                        detail: format!(
                            "{} exposed an ID3v2 tag through an audio format that cannot preserve its frame encoding",
                            path.display()
                        ),
                    })
                }
            },
            TagType::Id3v1 => legacy_id3v1_text(&mut reader, path)?,
            _ => LegacyTagText::default(),
        };
        let values = |legacy: Option<Vec<String>>, key: ItemKey| {
            stated_values(
                legacy.unwrap_or_else(|| tag.get_strings(key).map(String::from).collect()),
            )
        };
        Ok(FileTagRead {
            tags: FileTags {
                title: non_empty(legacy.title.or_else(|| tag.title().map(String::from))),
                track_artists: values(legacy.track_artists, ItemKey::TrackArtist),
                album_title: non_empty(
                    legacy.album_title.or_else(|| tag.album().map(String::from)),
                ),
                album_artists: values(legacy.album_artists, ItemKey::AlbumArtist),
                compilation: tag
                    .get_string(ItemKey::FlagCompilation)
                    .is_some_and(|flag| flag.trim() == "1"),
                years: tag_years(tag),
                numbers: TrackNumbers::of_tag(tag),
            },
            delivery: DeliveryTags {
                isrcs: values(None, ItemKey::Isrc),
                labels: values(legacy.labels, ItemKey::Label),
                copyrights: values(legacy.copyrights, ItemKey::CopyrightMessage),
                store,
            },
            embedded_cover: embedded_cover_from_tag(tag),
        })
    }
}

/// One file as lofty parses it: its tags, and what only its concrete format's
/// reading says.
struct ParsedFile {
    tagged: lofty::file::TaggedFile,
    id3v2: Id3v2Frames,
    /// Whether an MP4 file carries an atom iTunes writes only into a purchase.
    itunes_purchase: bool,
}

/// The ID3v2 frames a format keeps as they were written, which a tag that
/// names its own encoding wrongly is decoded from.
enum Id3v2Frames {
    /// The format keeps them: the legacy text of its ID3v2 tag, when it has
    /// one.
    Kept(Option<LegacyTagText>),
    /// The format keeps none of its own.
    Unkept,
}

/// Parse the file `reader` reads, of `file_type` as its extension says, once.
fn parse(
    reader: &mut BufReader<File>,
    path: &Path,
    file_type: Option<FileType>,
) -> Result<ParsedFile, ImportError> {
    use lofty::mp4::{AtomIdent, Mp4File};
    let options = ParseOptions::new().read_properties(false);
    let parsed = match file_type {
        Some(FileType::Aac) => parse_keeping_id3v2::<lofty::aac::AacFile>(reader, options),
        Some(FileType::Aiff) => parse_keeping_id3v2::<lofty::iff::aiff::AiffFile>(reader, options),
        Some(FileType::Ape) => parse_keeping_id3v2::<lofty::ape::ApeFile>(reader, options),
        Some(FileType::Flac) => parse_keeping_id3v2::<lofty::flac::FlacFile>(reader, options),
        Some(FileType::Mpeg) => parse_keeping_id3v2::<lofty::mpeg::MpegFile>(reader, options),
        Some(FileType::Mpc) => parse_keeping_id3v2::<lofty::musepack::MpcFile>(reader, options),
        Some(FileType::Wav) => parse_keeping_id3v2::<lofty::iff::wav::WavFile>(reader, options),
        Some(FileType::Mp4) => Mp4File::read_from(reader, options).map(|parsed| {
            let itunes_purchase = parsed.ilst().is_some_and(|ilst| {
                [*b"purd", *b"apID", *b"ownr"]
                    .iter()
                    .any(|atom| ilst.get(&AtomIdent::Fourcc(*atom)).is_some())
            });
            ParsedFile { tagged: parsed.into(), id3v2: Id3v2Frames::Unkept, itunes_purchase }
        }),
        other => match other {
            Some(file_type) => Probe::with_file_type(reader, file_type),
            None => Probe::new(reader),
        }
        .options(options)
        .read()
        .map(|tagged| ParsedFile {
            tagged,
            id3v2: Id3v2Frames::Unkept,
            itunes_purchase: false,
        }),
    };
    parsed.map_err(|error| ImportError::file_tags("read tags from", path, error))
}

/// The file parsed as the format `F`, which keeps its ID3v2 frames, with the
/// legacy text of its ID3v2 tag.
fn parse_keeping_id3v2<F>(
    reader: &mut BufReader<File>,
    options: ParseOptions,
) -> Result<ParsedFile, lofty::error::FileParseError>
where
    F: AudioFile + HasId3v2Tag + Into<lofty::file::TaggedFile>,
{
    let parsed = F::read_from(reader, options)?;
    let legacy = parsed.id3v2_tag().map(legacy_id3v2_text);
    Ok(ParsedFile {
        tagged: parsed.into(),
        id3v2: Id3v2Frames::Kept(legacy),
        itunes_purchase: false,
    })
}

/// `values` as a tag states them: each trimmed, blank ones dropped, and a
/// value written twice kept once.
fn stated_values(values: Vec<String>) -> Vec<String> {
    let mut stated: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        if let Some(value) = non_empty(Some(value)) {
            if !stated.contains(&value) {
                stated.push(value);
            }
        }
    }
    stated
}

/// The text of a tag that names its own encoding wrongly, decoded from its
/// bytes (see [`bae_util::text_encoding`]). A field is `None` where the tag holds
/// no such text for it, and the tag's own reading of it stands.
#[derive(Default)]
struct LegacyTagText {
    title: Option<String>,
    track_artists: Option<Vec<String>>,
    album_title: Option<String>,
    album_artists: Option<Vec<String>>,
    labels: Option<Vec<String>>,
    copyrights: Option<Vec<String>>,
}

/// The store a file's tags say sold it, if any — see [`StoreMarker`].
fn store_marker(itunes_purchase: bool, tag: Option<&lofty::tag::Tag>) -> Option<StoreMarker> {
    if itunes_purchase {
        return Some(StoreMarker::ITunesPurchase);
    }
    let bandcamp = tag.and_then(|tag| tag.get_string(ItemKey::Comment)).is_some_and(|comment| {
        comment.trim_start().starts_with("Visit ") && comment.contains(".bandcamp.com")
    });
    bandcamp.then_some(StoreMarker::Bandcamp)
}

trait HasId3v2Tag {
    fn id3v2_tag(&self) -> Option<&Id3v2Tag>;
}

macro_rules! impl_has_id3v2_tag {
    ($($type:path),+ $(,)?) => {
        $(
            impl HasId3v2Tag for $type {
                fn id3v2_tag(&self) -> Option<&Id3v2Tag> {
                    self.id3v2()
                }
            }
        )+
    };
}

impl_has_id3v2_tag!(
    lofty::aac::AacFile,
    lofty::ape::ApeFile,
    lofty::flac::FlacFile,
    lofty::iff::aiff::AiffFile,
    lofty::iff::wav::WavFile,
    lofty::mpeg::MpegFile,
    lofty::musepack::MpcFile,
);

/// The legacy text of an ID3v2 tag's frames.
fn legacy_id3v2_text(tag: &Id3v2Tag) -> LegacyTagText {
    let joined = |values: Option<Vec<String>>| values.map(|values| values.join("/"));
    LegacyTagText {
        title: joined(decoded_latin1_frame(tag, "TIT2")),
        track_artists: decoded_latin1_frame(tag, "TPE1"),
        album_title: joined(decoded_latin1_frame(tag, "TALB")),
        album_artists: decoded_latin1_frame(tag, "TPE2"),
        labels: decoded_latin1_frame(tag, "TPUB"),
        copyrights: decoded_latin1_frame(tag, "TCOP"),
    }
}

/// The legacy text of the ID3v1 tag at the end of the file `reader` reads.
fn legacy_id3v1_text(
    reader: &mut BufReader<File>,
    path: &Path,
) -> Result<LegacyTagText, ImportError> {
    reader
        .seek(SeekFrom::End(-128))
        .map_err(|error| ImportError::file_tags("seek to the ID3v1 tag in", path, error))?;
    let mut tag = [0_u8; 128];
    reader
        .read_exact(&mut tag)
        .map_err(|error| ImportError::file_tags("read the ID3v1 tag from", path, error))?;
    if tag[..3] != *b"TAG" {
        return Err(ImportError::FileTags {
            detail: format!(
                "{} was reported as ID3v1-tagged but its tag footer was absent",
                path.display()
            ),
        });
    }
    Ok(LegacyTagText {
        title: decoded_id3v1_field(&tag[3..33]),
        track_artists: decoded_id3v1_field(&tag[33..63]).map(|artist| vec![artist]),
        album_title: decoded_id3v1_field(&tag[63..93]),
        album_artists: None,
        labels: None,
        copyrights: None,
    })
}

fn decoded_id3v1_field(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    (end > 0).then(|| bae_util::text_encoding::decode_text(&bytes[..end]).text)
}

/// The values of the Latin-1 text frame `id`, each decoded from its bytes: an
/// ID3v2.4 frame separates its values with a NUL, an older one holds one.
fn decoded_latin1_frame(tag: &Id3v2Tag, id: &str) -> Option<Vec<String>> {
    let frame = tag.into_iter().find(|frame| frame.id_str() == id)?;
    let Frame::Text(text) = frame else {
        return None;
    };
    if text.encoding != TextEncoding::Latin1 {
        return None;
    }
    let bytes = text
        .value
        .chars()
        .map(|character| {
            u8::try_from(u32::from(character))
                .expect("Lofty's Latin-1 decoder emits only byte-valued scalars")
        })
        .collect::<Vec<_>>();
    if tag.original_version() == Id3v2Version::V4 {
        return Some(
            bytes
                .split(|byte| *byte == 0)
                .map(|value| bae_util::text_encoding::decode_text(value).text)
                .collect(),
        );
    }
    Some(vec![bae_util::text_encoding::decode_text(&bytes).text])
}

impl TrackNumbers {
    /// What `tag` states: its track number (Vorbis `TRACKNUMBER`, ID3 `TRCK`,
    /// MP4 `trkn`) and disc number (`DISCNUMBER`, `TPOS`, `disk`), without the
    /// totals some formats write beside them.
    pub(crate) fn of_tag(tag: &lofty::tag::Tag) -> Self {
        use lofty::tag::Accessor;
        Self {
            disc: tag.disk().filter(|number| *number > 0),
            track: tag.track().filter(|number| *number > 0),
        }
    }
}

pub(crate) fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn embedded_cover_from_tag(tag: &lofty::tag::Tag) -> Option<(Vec<u8>, ContentType)> {
    let mut pictures = tag.pictures().iter().filter_map(|picture| {
        picture.mime_type().and_then(image_content_type).map(|content_type| (picture, content_type))
    });
    let (picture, content_type) = pictures
        .clone()
        .find(|(picture, _)| picture.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| pictures.next())?;
    Some((picture.data().to_vec(), content_type))
}

pub(crate) fn image_content_type(mime: &lofty::picture::MimeType) -> Option<ContentType> {
    let content_type = ContentType::from_mime(mime.as_str());
    content_type.is_supported_cover().then_some(content_type)
}

/// Every year the dates `tag` states name, each once, in the order stated:
/// the recording date, the year, the release date and the original release
/// date, each as many times as the tag holds it.
fn tag_years(tag: &lofty::tag::Tag) -> Vec<u16> {
    let mut years = Vec::new();
    for key in
        [ItemKey::RecordingDate, ItemKey::Year, ItemKey::ReleaseDate, ItemKey::OriginalReleaseDate]
    {
        for value in tag.get_strings(key) {
            for year in years_in_date(value) {
                if !years.contains(&year) {
                    years.push(year);
                }
            }
        }
    }
    years
}

/// The years a date as a tag or CUE sheet writes it names: the first four
/// digits of each run of four or more — a bare year ("1970"), a date
/// ("1970-03-01", "19700301"), a range ("2000 / 2004"). A run of zeros names
/// no year.
pub(crate) fn years_in_date(date: &str) -> impl Iterator<Item = u16> + '_ {
    date.split(|c: char| !c.is_ascii_digit())
        .filter(|run| run.len() >= 4)
        .map(|run| run[..4].parse::<u16>().expect("four ASCII digits parse as u16"))
        .filter(|year| *year != 0)
}

#[cfg(test)]
#[path = "file_tag_reader_tests.rs"]
mod tests;
