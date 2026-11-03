use super::download as download_fact;
use super::*;
use bae_catalogs::CdProof;
use bae_types::pressing::StatedFormat;

fn per_medium(media: &[Option<Medium>]) -> StatedMedia {
    StatedMedia::PerMedium(media.to_vec())
}

fn formats(media: &[Option<Medium>]) -> StatedMedia {
    StatedMedia::Formats(
        media.iter().map(|medium| StatedFormat { medium: *medium, quantity: 1 }).collect(),
    )
}

fn cd_rip() -> AudioOrigin {
    AudioOrigin {
        source: Some(bae_catalogs::AudioSource::CdRip { proof: CdProof::RipLog, file: None }),
        not_cd_rate: None,
    }
}

fn not_cd_rate() -> AudioOrigin {
    AudioOrigin { source: None, not_cd_rate: Some(96_000) }
}

#[test]
fn a_rip_the_files_prove_or_a_matched_disc_id_is_a_cd() {
    assert_eq!(RippedFrom::of(&cd_rip(), false), RippedFrom::Cd);
    assert_eq!(RippedFrom::of(&AudioOrigin::default(), true), RippedFrom::Cd);
    assert_eq!(RippedFrom::of(&AudioOrigin::default(), false), RippedFrom::Unknown);
    assert_eq!(RippedFrom::of(&not_cd_rate(), false), RippedFrom::NotCd);
}

/// A download proves no carrier, at whatever rate.
#[test]
fn a_download_proves_no_carrier() {
    let download = bae_catalogs::AudioSource::Download(bae_catalogs::DownloadProof::DeliverySet);
    let at_cd_rate = AudioOrigin { source: Some(download.clone()), not_cd_rate: None };
    let at_96k = AudioOrigin { source: Some(download), not_cd_rate: Some(96_000) };
    assert_eq!(RippedFrom::of(&at_cd_rate, false), RippedFrom::Unknown);
    assert_eq!(RippedFrom::of(&at_96k, false), RippedFrom::Unknown);
}

/// A download agrees with a row stating a digital medium and disagrees
/// with one stating only carriers; a row naming none states nothing, and
/// a folder that is no download weighs no row this way.
#[test]
fn a_download_agrees_with_a_digital_release() {
    let download = AudioOrigin {
        source: Some(bae_catalogs::AudioSource::Download(bae_catalogs::DownloadProof::DeliverySet)),
        not_cd_rate: None,
    };
    let digital = per_medium(&[Some(Medium::Digital)]);
    let cd = per_medium(&[Some(Medium::Cd)]);
    assert_eq!(download_fact(&download, [&digital]), Fact::Agrees);
    assert_eq!(download_fact(&download, [&cd, &digital]), Fact::Agrees);
    assert_eq!(download_fact(&download, [&cd]), Fact::Disagrees);
    assert_eq!(download_fact(&download, [&formats(&[Some(Medium::Vinyl)])]), Fact::Disagrees);
    assert_eq!(download_fact(&download, [&StatedMedia::Undescribed]), Fact::StatesNothing);
    assert_eq!(download_fact(&download, [&per_medium(&[None])]), Fact::StatesNothing);
    assert_eq!(download_fact(&cd_rip(), [&digital]), Fact::StatesNothing);
    assert_eq!(download_fact(&AudioOrigin::default(), [&cd]), Fact::StatesNothing);
}

/// A CD rip rules out a row made only of carriers that play no CD audio,
/// and nothing that could hold one.
#[test]
fn a_cd_rip_rules_out_only_rows_with_no_cd_among_them() {
    let cd = RippedFrom::Cd;
    assert!(!cd.admits([&per_medium(&[Some(Medium::Vinyl)])]));
    assert!(!cd.admits([&formats(&[Some(Medium::Vinyl), Some(Medium::Cassette)])]));
    assert!(cd.admits([&per_medium(&[Some(Medium::Cd)])]));
    assert!(cd.admits([&per_medium(&[Some(Medium::Cd), Some(Medium::Dvd)])]));
    assert!(cd.admits([&per_medium(&[Some(Medium::Sacd)])]));
    assert!(cd.admits([&per_medium(&[Some(Medium::DualDisc)])]));
    assert!(cd.admits([&StatedMedia::Undescribed]));
    assert!(cd.admits([&per_medium(&[Some(Medium::Vinyl), None])]));
}

/// Audio no CD holds rules out a row made only of CDs, and nothing with
/// another carrier beside them.
#[test]
fn audio_off_a_cds_rate_rules_out_only_rows_of_cds() {
    let not_cd = RippedFrom::NotCd;
    assert!(!not_cd.admits([&per_medium(&[Some(Medium::Cd), Some(Medium::Cd)])]));
    assert!(not_cd.admits([&per_medium(&[Some(Medium::Vinyl)])]));
    assert!(not_cd.admits([&per_medium(&[Some(Medium::Cd), Some(Medium::Dvd)])]));
    assert!(not_cd.admits([&per_medium(&[Some(Medium::Sacd)])]));
    assert!(not_cd.admits([&StatedMedia::Undescribed]));
}

/// A row's records are one account: a record that describes nothing adds
/// nothing, and one that leaves a medium unnamed leaves the row open.
#[test]
fn a_rows_records_are_read_as_one_account() {
    let cd = RippedFrom::Cd;
    assert!(!cd.admits([&StatedMedia::Undescribed, &formats(&[Some(Medium::Vinyl)])]));
    assert!(cd.admits([&per_medium(&[Some(Medium::Vinyl)]), &formats(&[None])]));
    assert!(RippedFrom::Unknown.admits([&per_medium(&[Some(Medium::Vinyl)])]));
}

/// A row stating mono agrees with mono audio; one stating only stereo,
/// or nothing, does not, and two channels agree with no row.
#[test]
fn mono_audio_agrees_with_a_row_stating_mono() {
    use DiscogsDetail::{Mono, Stereo, T2TrackMono};
    assert!(agrees_with_mono(true, &[Mono]));
    assert!(agrees_with_mono(true, &[T2TrackMono]));
    assert!(agrees_with_mono(true, &[Mono, Stereo]));
    assert!(!agrees_with_mono(true, &[Stereo]));
    assert!(!agrees_with_mono(true, &[]));
    assert!(!agrees_with_mono(false, &[Mono]));
}
