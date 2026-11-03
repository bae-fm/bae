use super::*;
use bae_types::audio_format::AudioFormat;

fn format(sample_rate_hz: i64, bits_per_sample: Option<i64>, channels: i64) -> AudioFormat {
    AudioFormat {
        codec: "FLAC".to_string(),
        sample_rate_hz,
        bits_per_sample,
        bitrate_kbps: bits_per_sample.is_none().then_some(320),
        channels,
    }
}

/// Lossless audio at a rate a CD does not play at rules a CD out, mono or
/// not.
#[test]
fn lossless_audio_off_a_cds_rate_rules_it_out() {
    assert_eq!(rate_ruling_out_cd(&[format(96_000, Some(24), 1)]), Some(96_000));
    assert_eq!(
        rate_ruling_out_cd(&[format(48_000, Some(24), 2), format(96_000, Some(24), 2)]),
        Some(48_000)
    );
}

/// A CD's rate — whatever the channels or bit depth — lossy audio, one
/// file at a CD's rate among others, and no audio at all say nothing.
#[test]
fn audio_that_could_be_a_cds_says_nothing() {
    assert_eq!(rate_ruling_out_cd(&[format(44_100, Some(16), 2)]), None);
    assert_eq!(rate_ruling_out_cd(&[format(44_100, Some(24), 1)]), None);
    assert_eq!(rate_ruling_out_cd(&[format(48_000, None, 2)]), None);
    assert_eq!(
        rate_ruling_out_cd(&[format(96_000, Some(24), 2), format(44_100, Some(16), 2)]),
        None
    );
    assert_eq!(rate_ruling_out_cd(&[]), None);
}

#[test]
fn a_proof_reads_back_from_its_key() {
    for proof in [CdProof::RipLog, CdProof::AccurateRipReport, CdProof::RipperSheet] {
        assert_eq!(CdProof::from_key(proof.key()), Some(proof));
    }
}
