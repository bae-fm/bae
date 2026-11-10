use super::*;
use bae_records::DocumentKind;
use std::path::{Path, PathBuf};

fn scanned_non_audio(
    path: &str,
    role: crate::folder_scanner::FileRole,
) -> crate::folder_scanner::CandidateFile {
    crate::folder_scanner::CandidateFile {
        file: crate::folder_scanner::ScannedFile::new(
            PathBuf::from(path),
            Path::new(path)
                .file_name()
                .expect("test path has a file name")
                .to_string_lossy()
                .into_owned(),
            1,
            0,
        ),
        role,
    }
}

#[test]
fn resolve_file_content_type_maps_non_audio_by_extension() {
    // Non-audio files map straight from the extension hint — no probe, so
    // the paths need not exist.
    use crate::folder_scanner::FileRole;
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/cover.jpg", FileRole::Artwork)).unwrap(),
        ContentType::Jpeg
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/back.png", FileRole::Artwork)).unwrap(),
        ContentType::Png
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio(
            "/x/notes.txt",
            FileRole::Document { kind: DocumentKind::Text }
        ))
        .unwrap(),
        ContentType::PlainText
    );
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/booklet.pdf", FileRole::Other)).unwrap(),
        ContentType::Pdf
    );
    // An extension the hint can't classify becomes opaque binary.
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio("/x/data.bin", FileRole::Other)).unwrap(),
        ContentType::OctetStream
    );
}

/// A damaged image keeps its bytes but not its extension's claim: the library
/// reads a file stored as an image as a picture, and this one is not.
#[test]
fn a_damaged_image_is_stored_as_plain_bytes() {
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio(
            "/x/front.png",
            crate::folder_scanner::FileRole::DamagedImage
        ))
        .unwrap(),
        ContentType::OctetStream
    );
}

/// A rip folder can hold an extensionless file — a `README`, a bare
/// checksum — and the release carries it like any other. It is opaque
/// binary, not a reason to fail the import.
#[test]
fn resolve_file_content_type_without_an_extension_is_opaque_binary() {
    assert_eq!(
        resolve_file_content_type(&scanned_non_audio(
            "/x/README",
            crate::folder_scanner::FileRole::Other
        ))
        .unwrap(),
        ContentType::OctetStream
    );
}

#[test]
fn resolve_file_content_type_uses_scan_facts_for_new_audio_formats() {
    let fixture_root = bae_audio::test_fixtures::audio_format_dir();
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &fixture_root,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("scan audio-format fixtures");
    for (name, expected) in [
        ("placeholder-pcm.wav", bae_util::content_type::ContentType::Pcm),
        ("placeholder-pcm.aiff", bae_util::content_type::ContentType::Pcm),
        ("placeholder-opus.opus", bae_util::content_type::ContentType::Opus),
        ("placeholder-vorbis.ogg", bae_util::content_type::ContentType::Vorbis),
        ("placeholder-wavpack.wv", bae_util::content_type::ContentType::WavPack),
        ("placeholder-dsd.dsf", bae_util::content_type::ContentType::Dsd),
        ("placeholder-dsd.dff", bae_util::content_type::ContentType::Dsd),
    ] {
        let entry = files
            .files
            .iter()
            .find(|entry| entry.file.relative_path == name)
            .expect("fixture is present in the scan");
        assert_eq!(resolve_file_content_type(entry).unwrap(), expected, "{name}");
    }
}
