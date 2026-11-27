use super::*;

/// A stored value playback can't use is an error naming the track, never a
/// number wrapped into a different one.
#[test]
fn an_unusable_audio_format_value_is_an_error_naming_the_track() {
    for (column, value) in [
        ("sample_rate", 4_294_967_296_i64),
        ("sample_rate", -1),
        ("channels", 0),
        ("bits_per_sample", 33),
    ] {
        let mut stored = serde_json::json!({
            "id": "format-1", "track_id": "track-1", "content_type": "audio/flac",
            "pregap_samples": null, "generated_pregap_samples": null,
            "sample_rate": 44100, "bits_per_sample": 16, "channels": 2,
            "track_loudness_lufs": null, "track_peak_linear": null,
            "created_at": "2026-01-01T00:00:00Z"
        });
        stored[column] = value.into();
        let format: FormatJson = serde_json::from_value(stored).expect("a format's object");

        let message = format.decoded().expect_err(column).to_string();

        assert!(
            message.contains("track-1") && message.contains(column),
            "{column} = {value}: {message}"
        );
    }
}
