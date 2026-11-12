use super::*;

#[test]
fn mixed_source_audio_crosses_with_every_difference() {
    let summary = bae_records::source_audio::SourceAudioSummary::Mixed {
        differences: vec![
            bae_records::source_audio::SourceAudioDifference::Layout {
                layouts: vec![
                    bae_records::source_audio::SourceAudioLayout::Cue,
                    bae_records::source_audio::SourceAudioLayout::File,
                ],
            },
            bae_records::source_audio::SourceAudioDifference::Codec {
                codecs: vec!["FLAC".to_string(), "MP3".to_string()],
            },
        ],
    };

    let BridgeSourceAudioSummary::Mixed { differences } =
        BridgeSourceAudioSummary::from_core(summary)
    else {
        panic!("mixed source audio became uniform");
    };
    assert!(matches!(
        differences.as_slice(),
        [
            BridgeSourceAudioDifference::Layout { layouts },
            BridgeSourceAudioDifference::Codec { codecs },
        ] if matches!(
            layouts.as_slice(),
            [BridgeSourceAudioLayout::Cue, BridgeSourceAudioLayout::File]
        ) && codecs == &["FLAC", "MP3"]
    ));
}
