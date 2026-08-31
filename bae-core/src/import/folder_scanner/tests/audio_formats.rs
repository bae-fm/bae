fn mp3_with_id3_padding(padding: usize) -> Vec<u8> {
    let mut bytes = fake_mp3();
    assert_eq!(&bytes[..3], b"ID3", "MP3 fixture must begin with ID3v2");

    let tag_size = bytes[6..10]
        .iter()
        .fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    let padded_size = tag_size + padding;
    assert!(padded_size < 1 << 28, "ID3v2 size is a synchsafe integer");

    let audio_offset = 10 + tag_size;
    bytes.splice(audio_offset..audio_offset, std::iter::repeat_n(0, padding));
    for (index, shift) in [21, 14, 7, 0].into_iter().enumerate() {
        bytes[6 + index] = ((padded_size >> shift) & 0x7f) as u8;
    }
    bytes
}

#[test]
fn mp3_tag_sizes_do_not_make_uniform_audio_mixed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let album = temp_dir.path().join("Album");
    std::fs::create_dir(&album).unwrap();
    std::fs::write(album.join("01.mp3"), mp3_with_id3_padding(16 * 1024)).unwrap();
    std::fs::write(album.join("02.mp3"), mp3_with_id3_padding(64 * 1024)).unwrap();

    let candidates = scan_valid(temp_dir.path().to_path_buf());
    let candidate = candidates.first().expect("scanner returns the album");
    let summary = candidate
        .files
        .source_audio_summary()
        .expect("MP3 files have source-audio facts");

    let crate::album_detail::SourceAudioSummary::Uniform { descriptor } = summary else {
        panic!("equal MP3 streams with different tag sizes must be uniform");
    };
    assert_eq!(descriptor.format.bitrate_kbps, Some(320));
}

#[test]
fn opus_bitrate_comes_from_audio_packets_when_the_stream_has_no_declared_rate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let album = temp_dir.path().join("Album");
    std::fs::create_dir(&album).unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/test-fixtures/audio-format/placeholder-opus.opus"
        ),
        album.join("01.opus"),
    )
    .unwrap();

    let candidates = scan_valid(temp_dir.path().to_path_buf());
    let candidate = candidates.first().expect("scanner returns the album");
    let summary = candidate
        .files
        .source_audio_summary()
        .expect("Opus file has source-audio facts");

    let crate::album_detail::SourceAudioSummary::Uniform { descriptor } = summary else {
        panic!("one Opus stream must be uniform");
    };
    assert_eq!(descriptor.format.bitrate_kbps, Some(1));
}
