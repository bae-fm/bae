//! How the facts that speak to the object on the desk rank the rows: the
//! medium the folder's files prove or rule out, and what names one pressing
//! against what names every pressing cut from one master.

use super::*;
use crate::identify::MediumConflict;
use crate::pressing::{DiscogsDetail, Medium, StatedMedia};
use crate::signals::{AudioOrigin, AudioSource, CdProof, TextLine, TextOrigin};

type Outcome = (Findings, LibraryStatuses);
type Found = (MetadataResult, LibraryStatus);

/// A folder named after the album, with the catalog number every pressing
/// below carries.
fn folder() -> CandidateText {
    CandidateText::of(
        &[TextLine {
            text: "Artist One - Album One [L1-100]".to_string(),
            origin: TextOrigin::FolderName,
        }],
        &[],
        &[],
    )
}

/// One pressing of the album, made of `media`, carrying the folder's catalog
/// number.
fn pressing(release_id: &str, media: StatedMedia) -> Found {
    (
        MetadataResult {
            title: "Album One".to_string(),
            artist: Some("Artist One".to_string()),
            labels: vec![crate::pressing::ReleaseLabel::of(None, Some("L1-100"))],
            media,
            ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("rg-one"))
        },
        LibraryStatus::absent(release_id),
    )
}

fn made_of(media: &[Medium]) -> StatedMedia {
    StatedMedia::PerMedium(media.iter().copied().map(Some).collect())
}

const CD_RIP: AudioOrigin = AudioOrigin {
    source: Some(AudioSource::CdRip {
        proof: CdProof::RipLog,
        file: None,
    }),
    not_cd_rate: None,
};

const NOT_CD: AudioOrigin = AudioOrigin {
    source: None,
    not_cd_rate: Some(96_000),
};

const UNPROVEN: AudioOrigin = AudioOrigin {
    source: None,
    not_cd_rate: None,
};

/// The catalog lookup returned `rows`, and the folder's files say `origin`.
fn by_catalog(rows: Vec<Found>, origin: &AudioOrigin) -> Outcome {
    combine_results(
        LookupAnswers {
            catalog: rows,
            ..LookupAnswers::default()
        },
        &folder(),
        FolderAudio {
            origin,
            ..FolderAudio::UNPROVEN
        },
    )
}

fn offered(outcome: &Outcome) -> Vec<&str> {
    outcome
        .0
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect()
}

fn set_aside(outcome: &Outcome) -> Vec<&str> {
    outcome
        .0
        .narrowed_out
        .matches
        .iter()
        .map(|result| result.release_id.as_str())
        .collect()
}

/// A folder its rip log proves is a CD rip sets the vinyl pressing aside.
#[test]
fn a_cd_rip_sets_aside_a_vinyl_pressing() {
    let outcome = by_catalog(
        vec![
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cd", made_of(&[Medium::Cd])),
        ],
        &CD_RIP,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd"]);
    assert_eq!(set_aside(&outcome), vec!["rel-vinyl"]);
}

/// A pressing with a CD among its media, and one whose record says nothing
/// about its media, could each be what a CD rip was read off.
#[test]
fn a_cd_rip_keeps_a_cd_and_dvd_pressing_and_one_stating_nothing() {
    let outcome = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-cd-dvd", made_of(&[Medium::Cd, Medium::Dvd])),
            pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &CD_RIP,
    );
    assert_eq!(
        offered(&outcome),
        vec!["rel-cd", "rel-cd-dvd", "rel-undescribed"]
    );
    assert!(set_aside(&outcome).is_empty());
}

/// A folder whose files prove nothing about its medium sets nothing aside
/// for it.
#[test]
fn a_folder_that_proves_nothing_sets_nothing_aside() {
    let outcome = by_catalog(
        vec![
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cd", made_of(&[Medium::Cd])),
        ],
        &UNPROVEN,
    );
    assert_eq!(offered(&outcome), vec!["rel-vinyl", "rel-cd"]);
    assert!(set_aside(&outcome).is_empty());
}

/// Audio at a rate no CD plays at sets aside a pressing made only of CDs; a
/// record, a CD beside a DVD, and a pressing stating nothing stay.
#[test]
fn audio_no_cd_holds_sets_aside_a_cd_pressing() {
    let outcome = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cd-dvd", made_of(&[Medium::Cd, Medium::Dvd])),
            pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &NOT_CD,
    );
    assert_eq!(
        offered(&outcome),
        vec!["rel-vinyl", "rel-cd-dvd", "rel-undescribed"]
    );
    assert_eq!(set_aside(&outcome), vec!["rel-cd"]);
}

/// A download at a rate no CD plays at, the way a store delivers one.
const HI_RES_DOWNLOAD: AudioOrigin = AudioOrigin {
    source: Some(AudioSource::Download(
        crate::signals::DownloadProof::DeliverySet,
    )),
    not_cd_rate: Some(96_000),
};

/// A download is a copy of the digital release, which is offered over a
/// pressing naming no carrier and a physical one. A physical pressing is
/// outranked, never ruled out by the rate, so no conflict is stated even when
/// every pressing is a CD.
#[test]
fn a_download_offers_the_digital_release() {
    let outcome = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-digital", made_of(&[Medium::Digital])),
            pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &HI_RES_DOWNLOAD,
    );
    assert_eq!(offered(&outcome), vec!["rel-digital"]);
    assert_eq!(set_aside(&outcome), vec!["rel-cd", "rel-undescribed"]);

    let outcome = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &HI_RES_DOWNLOAD,
    );
    assert_eq!(offered(&outcome), vec!["rel-undescribed"]);

    let outcome = by_catalog(
        vec![pressing("rel-cd", made_of(&[Medium::Cd])), {
            let (mut result, status) = pressing("rel-cd-2", made_of(&[Medium::Cd]));
            result.packaging = Some(crate::pressing::Packaging::Digipak);
            (result, status)
        }],
        &HI_RES_DOWNLOAD,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd", "rel-cd-2"]);
    assert_eq!(outcome.0.medium_conflict, None);
}

/// A matched disc ID proves a CD as surely as a rip log, so the vinyl
/// pressing is set aside.
#[test]
fn a_matched_disc_id_proves_a_cd() {
    let vinyl = pressing("rel-vinyl", made_of(&[Medium::Vinyl]));
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![pressing("rel-cd", made_of(&[Medium::Cd]))],
            barcode: vec![vinyl.clone()],
            catalog: vec![vinyl],
            ..LookupAnswers::default()
        },
        &folder(),
        FolderAudio::UNPROVEN,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd"]);
    assert_eq!(set_aside(&outcome), vec!["rel-vinyl"]);
}

/// Every row contradicted is still every row the run found: the list is
/// shortened, never emptied.
#[test]
fn rows_all_contradicted_all_stay() {
    let outcome = by_catalog(
        vec![
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            pressing("rel-cassette", made_of(&[Medium::Cassette])),
        ],
        &CD_RIP,
    );
    assert_eq!(offered(&outcome), vec!["rel-vinyl", "rel-cassette"]);
    assert!(set_aside(&outcome).is_empty());
}

/// A barcode and catalog number name one pressing where a disc ID names every
/// pressing cut from one master, so theirs outranks the disc ID's.
#[test]
fn the_pressing_the_barcode_and_catalog_number_name_outranks_the_disc_id_s() {
    let named = pressing("rel-named", made_of(&[Medium::Cd]));
    let mut other = pressing("rel-other", made_of(&[Medium::Cd]));
    other.0.labels = vec![crate::pressing::ReleaseLabel::of(None, Some("L1-999"))];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![other],
            barcode: vec![named],
            ..LookupAnswers::default()
        },
        &folder(),
        FolderAudio {
            origin: &CD_RIP,
            mono: false,
            track_count: 0,
            registered_in: None,
            track_titles: &[],
        },
    );
    assert_eq!(offered(&outcome), vec!["rel-named"]);
    assert_eq!(set_aside(&outcome), vec!["rel-other"]);
}

/// A sleeve saying where it was made offers the pressing released there; a
/// label's address beside it says nothing.
#[test]
fn a_sleeve_saying_where_it_was_made_offers_that_pressing() {
    let line = |text: &str, origin| TextLine {
        text: text.to_string(),
        origin,
    };
    let text = CandidateText::of(
        &[
            line("2010. Artist One - Album One", TextOrigin::FolderName),
            line(
                "Placeholder Records, 100 Placeholder Drive, Beverly Hills, CA 90210.",
                TextOrigin::Artwork,
            ),
            line(
                "Unauthorised copying prohibited. Made in the EU. LC 00000. BIEM/SABAM.",
                TextOrigin::Artwork,
            ),
        ],
        &[],
        &[],
    );
    let released_in = |release_id: &str, area: &str| {
        let (result, status) = pressing(release_id, made_of(&[Medium::Cd]));
        (
            MetadataResult {
                area: Some(crate::pressing::area(area)),
                year: Some(2010),
                labels: Vec::new(),
                ..result
            },
            status,
        )
    };
    let outcome = combine_results(
        LookupAnswers {
            barcode: vec![released_in("rel-us", "US"), released_in("rel-europe", "XE")],
            ..LookupAnswers::default()
        },
        &text,
        FolderAudio::UNPROVEN,
    );
    assert_eq!(offered(&outcome), vec!["rel-europe"]);
    assert_eq!(set_aside(&outcome), vec!["rel-us"]);
}

/// Stating no pressing's country, or both, leaves the two tied.
#[test]
fn stating_no_country_or_both_leaves_the_pressings_tied() {
    let line = |text: &str| TextLine {
        text: text.to_string(),
        origin: TextOrigin::Artwork,
    };
    let released_in = |release_id: &str, area: &str| {
        let (result, status) = pressing(release_id, made_of(&[Medium::Cd]));
        (
            MetadataResult {
                area: Some(crate::pressing::area(area)),
                labels: Vec::new(),
                ..result
            },
            status,
        )
    };
    for sleeve in [
        vec![line("Artist One - Album One")],
        vec![
            line("Artist One - Album One"),
            line("Made in the EU."),
            line("Distributed in the United States"),
        ],
    ] {
        let outcome = combine_results(
            LookupAnswers {
                barcode: vec![released_in("rel-us", "US"), released_in("rel-europe", "XE")],
                ..LookupAnswers::default()
            },
            &CandidateText::of(&sleeve, &[], &[]),
            FolderAudio::UNPROVEN,
        );
        assert_eq!(offered(&outcome).len(), 2, "{sleeve:?}");
        assert!(set_aside(&outcome).is_empty(), "{sleeve:?}");
    }
}

/// Rows the folder rules out all stay when there is nothing else, and the
/// verdict carries the conflict.
#[test]
fn rows_the_folder_rules_out_carry_the_conflict() {
    let every_cd = by_catalog(
        vec![
            pressing("rel-cd-1", made_of(&[Medium::Cd])),
            pressing("rel-cd-2", made_of(&[Medium::Cd, Medium::Cd])),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&every_cd), vec!["rel-cd-1", "rel-cd-2"]);
    assert_eq!(every_cd.0.medium_conflict, Some(MediumConflict::NotCdAudio));
    let every_vinyl = by_catalog(
        vec![pressing("rel-vinyl", made_of(&[Medium::Vinyl]))],
        &CD_RIP,
    );
    assert_eq!(every_vinyl.0.medium_conflict, Some(MediumConflict::CdRip));
}

/// A row the folder admits leads, and the verdict carries no conflict.
#[test]
fn an_admitted_row_leads_with_no_conflict() {
    let mixed = by_catalog(
        vec![
            pressing("rel-cd", made_of(&[Medium::Cd])),
            pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&mixed), vec!["rel-vinyl"]);
    assert_eq!(mixed.0.medium_conflict, None);
}

/// Two pressings of one LP the lookups name alike, one stated mono and one
/// stereo: a folder of one-channel audio agrees with the mono one, which is
/// offered as the tiebreak, and the stereo one waits behind the disclosure —
/// set aside, not ruled out.
#[test]
fn mono_audio_offers_the_pressing_stated_mono() {
    let lp = |release_id: &str, details: Vec<DiscogsDetail>| {
        let (result, status) = pressing(release_id, made_of(&[Medium::Vinyl]));
        (
            MetadataResult {
                discogs_details: details,
                ..result
            },
            status,
        )
    };
    let outcome = combine_results(
        LookupAnswers {
            catalog: vec![
                lp("rel-stereo", vec![DiscogsDetail::Stereo]),
                lp("rel-mono", vec![DiscogsDetail::Mono]),
            ],
            ..LookupAnswers::default()
        },
        &folder(),
        MONO_FILES,
    );
    assert_eq!(offered(&outcome), vec!["rel-mono"]);
    assert_eq!(set_aside(&outcome), vec!["rel-stereo"]);
    assert_eq!(outcome.0.medium_conflict, None);
}

/// A pressing of `media` that Discogs lists as stereo.
fn stated_stereo(release_id: &str, media: StatedMedia) -> Found {
    let (result, status) = pressing(release_id, media);
    (
        MetadataResult {
            discogs_details: vec![DiscogsDetail::Stereo],
            ..result
        },
        status,
    )
}

const MONO_FILES: FolderAudio<'static> = FolderAudio {
    origin: &UNPROVEN,
    mono: true,
    track_count: 0,
    registered_in: None,
    track_titles: &[],
};

/// Catalogs list mono pressings as stereo, so one-channel files rule no row
/// out: the stereo-listed pressing the barcode and the catalog number both
/// returned outranks one the title search alone found, even stated mono.
#[test]
fn mono_audio_does_not_outrank_what_the_lookups_agree_on() {
    let stereo = stated_stereo("rel-stereo", made_of(&[Medium::Vinyl]));
    let (mono_result, mono_status) = pressing("rel-mono", made_of(&[Medium::Vinyl]));
    let by_title = (
        MetadataResult {
            labels: Vec::new(),
            discogs_details: vec![DiscogsDetail::Mono],
            ..mono_result
        },
        mono_status,
    );
    let outcome = combine_results(
        LookupAnswers {
            barcode: vec![stereo.clone()],
            catalog: vec![stereo],
            search: vec![by_title],
            ..LookupAnswers::default()
        },
        &folder(),
        MONO_FILES,
    );
    assert_eq!(offered(&outcome), vec!["rel-stereo"]);
    assert_eq!(set_aside(&outcome), vec!["rel-mono"]);
    assert_eq!(outcome.0.medium_conflict, None);
}

/// Where every pressing is listed as stereo, one-channel files leave them all
/// offered with no conflict, so a single one is auto-importable like any other.
#[test]
fn mono_audio_against_stereo_listings_can_still_be_ready() {
    let (result, status) = stated_stereo("rel-stereo", made_of(&[Medium::Vinyl]));
    let listed = (
        MetadataResult {
            source_tracks: Some(crate::import::search::SourceTracks::Listed { count: 11 }),
            ..result
        },
        status,
    );
    let (findings, _) = combine_results(
        LookupAnswers {
            barcode: vec![listed],
            ..LookupAnswers::default()
        },
        &folder(),
        FolderAudio {
            track_count: 11,
            ..MONO_FILES
        },
    );
    assert_eq!(findings.medium_conflict, None);
    let verdict = crate::identify::TerminalVerdict::Found {
        findings,
        track_count: 11,
        ledger: None,
    };
    assert_eq!(
        crate::identify::VerdictSummary::of(&verdict, false).judgement(),
        (true, None)
    );
}

/// Of two releases on the folder's label and country, the one whose title and
/// artist the folder states is offered.
#[test]
fn the_album_the_folder_names_outranks_another_on_the_same_label() {
    let text = CandidateText::of(
        &[TextLine {
            text: "Artist One - Album One (Label One, US)".to_string(),
            origin: TextOrigin::FolderName,
        }],
        &[],
        &[],
    );
    let on_label = |release_id: &str, title: &str, artist: &str| {
        let (result, status) = pressing(release_id, made_of(&[Medium::Vinyl]));
        (
            MetadataResult {
                title: title.to_string(),
                artist: Some(artist.to_string()),
                labels: vec![crate::pressing::ReleaseLabel::of(Some("Label One"), None)],
                area: Some(crate::pressing::area("US")),
                ..result
            },
            status,
        )
    };
    let outcome = combine_results(
        LookupAnswers {
            barcode: vec![
                on_label("rel-other", "Album Two", "Artist Two"),
                on_label("rel-named", "Album One", "Artist One"),
            ],
            ..LookupAnswers::default()
        },
        &text,
        FolderAudio::UNPROVEN,
    );
    assert_eq!(offered(&outcome), vec!["rel-named"]);
    assert_eq!(set_aside(&outcome), vec!["rel-other"]);
}

/// Three pressings of the album, tied on everything but the country each was
/// released in.
fn released_in(countries: &[(&str, &str)]) -> Vec<Found> {
    countries
        .iter()
        .map(|(release_id, country)| {
            let (result, status) = pressing(release_id, made_of(&[Medium::Cd]));
            (
                MetadataResult {
                    area: Some(crate::pressing::area(country)),
                    ..result
                },
                status,
            )
        })
        .collect()
}

fn registered_in(country: Option<&str>) -> FolderAudio<'static> {
    FolderAudio {
        registered_in: country.map(crate::pressing::area),
        ..FolderAudio::UNPROVEN
    }
}

/// Where the tracks' recordings were registered breaks a tie between rows of
/// different countries: the pressing released there ranks first.
#[test]
fn the_country_the_recordings_were_registered_in_breaks_a_tie() {
    let outcome = combine_results(
        LookupAnswers {
            barcode: released_in(&[("rel-de", "DE"), ("rel-it", "IT"), ("rel-fr", "FR")]),
            ..LookupAnswers::default()
        },
        &folder(),
        registered_in(Some("IT")),
    );
    assert_eq!(offered(&outcome), vec!["rel-it"]);
    assert_eq!(set_aside(&outcome), vec!["rel-de", "rel-fr"]);
}

/// Recordings registered in no one country leave the tie standing.
#[test]
fn recordings_registered_nowhere_in_particular_break_no_tie() {
    let outcome = combine_results(
        LookupAnswers {
            barcode: released_in(&[("rel-de", "DE"), ("rel-it", "IT"), ("rel-fr", "FR")]),
            ..LookupAnswers::default()
        },
        &folder(),
        registered_in(None),
    );
    assert_eq!(offered(&outcome), vec!["rel-de", "rel-it", "rel-fr"]);
}

/// The folder naming a country outranks where the recordings were registered.
#[test]
fn the_folder_naming_a_country_outranks_where_the_recordings_were_registered() {
    let text = CandidateText::of(
        &[TextLine {
            text: "Artist One - Album One [L1-100] (Germany)".to_string(),
            origin: TextOrigin::FolderName,
        }],
        &[],
        &[],
    );
    let outcome = combine_results(
        LookupAnswers {
            barcode: released_in(&[("rel-de", "DE"), ("rel-it", "IT")]),
            ..LookupAnswers::default()
        },
        &text,
        registered_in(Some("IT")),
    );
    assert_eq!(offered(&outcome), vec!["rel-de"]);
}

/// A pressing the ISRC search returned stands on one more lookup than its
/// sibling the title search alone found.
#[test]
fn the_isrcs_lend_a_row_their_lookup() {
    let outcome = combine_results(
        LookupAnswers {
            isrc: vec![pressing("rel-by-isrc", StatedMedia::Undescribed)],
            search: vec![
                pressing("rel-by-isrc", StatedMedia::Undescribed),
                pressing("rel-by-title", StatedMedia::Undescribed),
            ],
            ..LookupAnswers::default()
        },
        &folder(),
        FolderAudio::UNPROVEN,
    );
    assert_eq!(offered(&outcome), vec!["rel-by-isrc"]);
    assert_eq!(set_aside(&outcome), vec!["rel-by-title"]);
    assert!(outcome.0.provenance[0].by_isrc && outcome.0.provenance[0].by_search);
}
