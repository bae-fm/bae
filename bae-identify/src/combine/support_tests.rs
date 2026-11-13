//! How the facts that speak to the object on the desk rank the rows: the
//! medium the folder's files prove or rule out, and what names one pressing
//! against what names every pressing cut from one master.
//!
//! How the catalogs' notes break a tie between pressings that agree on
//! everything else: the folder naming a word only one row's notes write.
//! How the folder's years rank pressings of one album: a year the album first
//! came out in confirms the album, and a later one names the edition.

use super::*;
use crate::signals::{TextLine, TextOrigin};
use crate::MediumConflict;
use bae_catalogs::search::SourceTracks;
use bae_catalogs::{AudioOrigin, AudioSource, CdProof};
use bae_types::pressing::{DiscogsDetail, Medium, StatedMedia};

type Outcome = Findings;
type Found = MetadataResult;

/// A folder named after the album, with the catalog number every pressing
/// below carries.
fn catalog_folder() -> CandidateText {
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
fn stated_pressing(release_id: &str, media: StatedMedia) -> Found {
    MetadataResult {
        title: "Album One".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist One")),
        labels: vec![bae_types::pressing::ReleaseLabel::of(None, Some("L1-100"))],
        media,
        ..MetadataResult::for_test(LookupCatalog::MusicBrainz, release_id, Some("rg-one"))
    }
}

fn made_of(media: &[Medium]) -> StatedMedia {
    StatedMedia::PerMedium(media.iter().copied().map(Some).collect())
}

const CD_RIP: AudioOrigin = AudioOrigin {
    source: Some(AudioSource::CdRip { proof: CdProof::RipLog, file: None }),
    not_cd_rate: None,
};

const NOT_CD: AudioOrigin = AudioOrigin { source: None, not_cd_rate: Some(96_000) };

const UNPROVEN: AudioOrigin = AudioOrigin { source: None, not_cd_rate: None };

/// The catalog lookup returned `rows`, and the folder's files say `origin`.
fn by_catalog(rows: Vec<Found>, origin: &AudioOrigin) -> Outcome {
    combine_results(
        LookupAnswers { catalog: rows, ..LookupAnswers::default() },
        &catalog_folder(),
        FolderAudio { origin, ..FolderAudio::UNPROVEN },
    )
}

fn offered(outcome: &Outcome) -> Vec<&str> {
    outcome.matches.iter().map(|result| result.release_id.as_str()).collect()
}

fn set_aside(outcome: &Outcome) -> Vec<&str> {
    outcome.narrowed_out.matches.iter().map(|result| result.release_id.as_str()).collect()
}

/// A folder its rip log proves is a CD rip sets the vinyl pressing aside.
#[test]
fn a_cd_rip_sets_aside_a_vinyl_pressing() {
    let outcome = by_catalog(
        vec![
            stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
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
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
            stated_pressing("rel-cd-dvd", made_of(&[Medium::Cd, Medium::Dvd])),
            stated_pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &CD_RIP,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd", "rel-cd-dvd", "rel-undescribed"]);
    assert!(set_aside(&outcome).is_empty());
}

/// A folder whose files prove nothing about its medium sets nothing aside
/// for it.
#[test]
fn a_folder_that_proves_nothing_sets_nothing_aside() {
    let outcome = by_catalog(
        vec![
            stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
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
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
            stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            stated_pressing("rel-cd-dvd", made_of(&[Medium::Cd, Medium::Dvd])),
            stated_pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&outcome), vec!["rel-vinyl", "rel-cd-dvd", "rel-undescribed"]);
    assert_eq!(set_aside(&outcome), vec!["rel-cd"]);
}

/// A download at a rate no CD plays at, the way a store delivers one.
const HI_RES_DOWNLOAD: AudioOrigin = AudioOrigin {
    source: Some(AudioSource::Download(bae_catalogs::DownloadProof::DeliverySet)),
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
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
            stated_pressing("rel-digital", made_of(&[Medium::Digital])),
            stated_pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &HI_RES_DOWNLOAD,
    );
    assert_eq!(offered(&outcome), vec!["rel-digital"]);
    assert_eq!(set_aside(&outcome), vec!["rel-cd", "rel-undescribed"]);

    let outcome = by_catalog(
        vec![
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
            stated_pressing("rel-undescribed", StatedMedia::Undescribed),
        ],
        &HI_RES_DOWNLOAD,
    );
    assert_eq!(offered(&outcome), vec!["rel-undescribed"]);

    let outcome = by_catalog(
        vec![stated_pressing("rel-cd", made_of(&[Medium::Cd])), {
            let mut result = stated_pressing("rel-cd-2", made_of(&[Medium::Cd]));
            result.packaging = Some(bae_types::pressing::Packaging::Digipak);
            result
        }],
        &HI_RES_DOWNLOAD,
    );
    assert_eq!(offered(&outcome), vec!["rel-cd", "rel-cd-2"]);
    assert_eq!(outcome.medium_conflict, None);
}

/// A matched disc ID proves a CD as surely as a rip log, so the vinyl
/// pressing is set aside.
#[test]
fn a_matched_disc_id_proves_a_cd() {
    let vinyl = stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl]));
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![vec![stated_pressing("rel-cd", made_of(&[Medium::Cd]))]],
            barcode: vec![vinyl.clone()],
            catalog: vec![vinyl],
            ..LookupAnswers::default()
        },
        &catalog_folder(),
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
            stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
            stated_pressing("rel-cassette", made_of(&[Medium::Cassette])),
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
    let named = stated_pressing("rel-named", made_of(&[Medium::Cd]));
    let mut other = stated_pressing("rel-other", made_of(&[Medium::Cd]));
    other.labels = vec![bae_types::pressing::ReleaseLabel::of(None, Some("L1-999"))];
    let outcome = combine_results(
        LookupAnswers {
            disc_id: vec![vec![other]],
            barcode: vec![named],
            ..LookupAnswers::default()
        },
        &catalog_folder(),
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
    let line = |text: &str, origin| TextLine { text: text.to_string(), origin };
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
        let result = stated_pressing(release_id, made_of(&[Medium::Cd]));
        MetadataResult {
            area: Some(bae_types::pressing::area(area)),
            year: Some(2010),
            labels: Vec::new(),
            ..result
        }
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
    let line = |text: &str| TextLine { text: text.to_string(), origin: TextOrigin::Artwork };
    let released_in = |release_id: &str, area: &str| {
        let result = stated_pressing(release_id, made_of(&[Medium::Cd]));
        MetadataResult { area: Some(bae_types::pressing::area(area)), labels: Vec::new(), ..result }
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
            stated_pressing("rel-cd-1", made_of(&[Medium::Cd])),
            stated_pressing("rel-cd-2", made_of(&[Medium::Cd, Medium::Cd])),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&every_cd), vec!["rel-cd-1", "rel-cd-2"]);
    assert_eq!(every_cd.medium_conflict, Some(MediumConflict::NotCdAudio));
    let every_vinyl =
        by_catalog(vec![stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl]))], &CD_RIP);
    assert_eq!(every_vinyl.medium_conflict, Some(MediumConflict::CdRip));
}

/// A row the folder admits leads, and the verdict carries no conflict.
#[test]
fn an_admitted_row_leads_with_no_conflict() {
    let mixed = by_catalog(
        vec![
            stated_pressing("rel-cd", made_of(&[Medium::Cd])),
            stated_pressing("rel-vinyl", made_of(&[Medium::Vinyl])),
        ],
        &NOT_CD,
    );
    assert_eq!(offered(&mixed), vec!["rel-vinyl"]);
    assert_eq!(mixed.medium_conflict, None);
}

/// Two pressings of one LP the lookups name alike, one stated mono and one
/// stereo: a folder of one-channel audio agrees with the mono one, which is
/// offered as the tiebreak, and the stereo one waits behind the disclosure —
/// set aside, not ruled out.
#[test]
fn mono_audio_offers_the_pressing_stated_mono() {
    let lp = |release_id: &str, details: Vec<DiscogsDetail>| {
        let result = stated_pressing(release_id, made_of(&[Medium::Vinyl]));
        MetadataResult { discogs_details: details, ..result }
    };
    let outcome = combine_results(
        LookupAnswers {
            catalog: vec![
                lp("rel-stereo", vec![DiscogsDetail::Stereo]),
                lp("rel-mono", vec![DiscogsDetail::Mono]),
            ],
            ..LookupAnswers::default()
        },
        &catalog_folder(),
        MONO_FILES,
    );
    assert_eq!(offered(&outcome), vec!["rel-mono"]);
    assert_eq!(set_aside(&outcome), vec!["rel-stereo"]);
    assert_eq!(outcome.medium_conflict, None);
}

/// A pressing of `media` that Discogs lists as stereo.
fn stated_stereo(release_id: &str, media: StatedMedia) -> Found {
    let result = stated_pressing(release_id, media);
    MetadataResult { discogs_details: vec![DiscogsDetail::Stereo], ..result }
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
    let mono_result = stated_pressing("rel-mono", made_of(&[Medium::Vinyl]));
    let by_title = MetadataResult {
        labels: Vec::new(),
        discogs_details: vec![DiscogsDetail::Mono],
        ..mono_result
    };
    let outcome = combine_results(
        LookupAnswers {
            barcode: vec![stereo.clone()],
            catalog: vec![stereo],
            search: vec![by_title],
            ..LookupAnswers::default()
        },
        &catalog_folder(),
        MONO_FILES,
    );
    assert_eq!(offered(&outcome), vec!["rel-stereo"]);
    assert_eq!(set_aside(&outcome), vec!["rel-mono"]);
    assert_eq!(outcome.medium_conflict, None);
}

/// Where every pressing is listed as stereo, one-channel files leave them all
/// offered with no conflict, so a single one is auto-importable like any other.
#[test]
fn mono_audio_against_stereo_listings_can_still_be_ready() {
    let result = stated_stereo("rel-stereo", made_of(&[Medium::Vinyl]));
    let listed = MetadataResult {
        source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 11 }),
        ..result
    };
    let findings = combine_results(
        LookupAnswers { barcode: vec![listed], ..LookupAnswers::default() },
        &catalog_folder(),
        FolderAudio { track_count: 11, ..MONO_FILES },
    );
    assert_eq!(findings.medium_conflict, None);
    let verdict = crate::TerminalVerdict::Found { findings, track_count: 11, ledger: None };
    assert_eq!(crate::VerdictSummary::of(&verdict, false).judgement(), (true, None));
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
        let result = stated_pressing(release_id, made_of(&[Medium::Vinyl]));
        MetadataResult {
            title: title.to_string(),
            artist: Some(bae_types::credits::CreditLine::of_name(artist)),
            labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label One"), None)],
            area: Some(bae_types::pressing::area("US")),
            ..result
        }
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
            let result = stated_pressing(release_id, made_of(&[Medium::Cd]));
            MetadataResult { area: Some(bae_types::pressing::area(country)), ..result }
        })
        .collect()
}

fn registered_in(country: Option<&str>) -> FolderAudio<'static> {
    FolderAudio { registered_in: country.map(bae_types::pressing::area), ..FolderAudio::UNPROVEN }
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
        &catalog_folder(),
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
        &catalog_folder(),
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
            isrc: vec![stated_pressing("rel-by-isrc", StatedMedia::Undescribed)],
            search: vec![
                stated_pressing("rel-by-isrc", StatedMedia::Undescribed),
                stated_pressing("rel-by-title", StatedMedia::Undescribed),
            ],
            ..LookupAnswers::default()
        },
        &catalog_folder(),
        FolderAudio::UNPROVEN,
    );
    assert_eq!(offered(&outcome), vec!["rel-by-isrc"]);
    assert_eq!(set_aside(&outcome), vec!["rel-by-title"]);
}

fn folder(line: &str) -> CandidateText {
    CandidateText::of(
        &[TextLine { text: line.to_string(), origin: TextOrigin::FolderName }],
        &[],
        &[],
    )
}

/// A pressing of Album One on Label One, L1-100, whose read document writes
/// `notes`, each in its own packaging so no two look alike.
fn noted_pressing(release_id: &str, notes: &[&str]) -> MetadataResult {
    MetadataResult {
        title: "Album One".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist One")),
        year: Some(1987),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label One"), Some("L1-100"))],
        notes: notes.iter().map(|note| note.to_string()).collect(),
        packaging: Some(match release_id {
            "rel-alpha" => bae_types::pressing::Packaging::JewelCase,
            "rel-beta" => bae_types::pressing::Packaging::Digipak,
            "rel-gamma" => bae_types::pressing::Packaging::GatefoldCover,
            _ => bae_types::pressing::Packaging::CardboardSleeve,
        }),
        ..MetadataResult::for_test(LookupCatalog::MusicBrainz, release_id, Some("rg-album-one"))
    }
}

fn noted_by_catalog(results: Vec<MetadataResult>, text: &CandidateText) -> Outcome {
    combine_results(
        LookupAnswers { catalog: results, ..LookupAnswers::default() },
        text,
        FolderAudio::UNPROVEN,
    )
}

/// Two pressings alike in every field: the folder naming a word only the
/// second's notes write offers that one.
#[test]
fn a_word_only_one_row_s_notes_write_offers_that_row() {
    let outcome = noted_by_catalog(
        vec![
            noted_pressing("rel-alpha", &["Pressed By Plant Alpha"]),
            noted_pressing("rel-beta", &["Made In Nordland By Plant Beta"]),
        ],
        &folder("1987 Nordland Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-beta"]);
}

/// The folder naming a word of each row's own notes sets neither apart.
#[test]
fn words_from_both_rows_notes_keep_the_tie() {
    let outcome = noted_by_catalog(
        vec![
            noted_pressing("rel-alpha", &["Pressed By Plant Alpha"]),
            noted_pressing("rel-beta", &["Made In Nordland By Plant Beta"]),
        ],
        &folder("1987 Nordland Alpha L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-alpha", "rel-beta"]);
}

/// A word both rows' notes write tells neither apart, however the folder
/// names it.
#[test]
fn a_word_every_row_s_notes_write_gives_no_point() {
    let outcome = noted_by_catalog(
        vec![
            noted_pressing("rel-alpha", &["Pressed By Plant Alpha"]),
            noted_pressing("rel-beta", &["Pressed By Plant Beta"]),
            noted_pressing("rel-gamma", &["Plant Gamma"]),
        ],
        &folder("1987 Plant L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-alpha", "rel-beta", "rel-gamma"]);
}

/// A matrix inscription writing the catalog number the folder names gives no
/// point: numbers in notes are the codes every pressing of the number
/// carries, which one catalog entry transcribes and a look-alike's may not.
#[test]
fn numbers_in_the_notes_give_no_point() {
    let outcome = noted_by_catalog(
        vec![
            noted_pressing("rel-alpha", &["1 100 L1-100 PA-01"]),
            noted_pressing("rel-beta", &["Made In Nordland By Plant Beta"]),
        ],
        &folder("1987 Nordland Label One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-beta"]);
}

/// The notes only break ties: a row the folder's catalog number names is
/// offered over one whose notes the folder names.
#[test]
fn the_notes_never_outrank_an_earlier_field() {
    let numbered = noted_pressing("rel-alpha", &["Pressed By Plant Alpha"]);
    let unnumbered = MetadataResult {
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label One"), None)],
        ..noted_pressing("rel-beta", &["Made In Nordland By Plant Beta"])
    };
    let outcome =
        noted_by_catalog(vec![numbered, unnumbered], &folder("1987 Nordland Label One L1-100"));
    assert_eq!(offered(&outcome), vec!["rel-alpha"]);
}

/// Where nothing else tells two look-alike rows apart, the dated one is
/// offered and the undated one set aside.
#[test]
fn an_undated_look_alike_folds_below_a_dated_one() {
    let dated = noted_pressing("rel-dated", &[]);
    let undated = MetadataResult { year: None, ..noted_pressing("rel-undated", &[]) };
    let outcome = noted_by_catalog(vec![undated, dated], &folder("Label One L1-100"));
    assert_eq!(offered(&outcome), vec!["rel-dated"]);
    assert_eq!(
        outcome
            .narrowed_out
            .matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-undated"]
    );
}

/// Stating a year only breaks a tie: an undated row better on an earlier
/// field is offered over a dated one.
#[test]
fn stating_a_year_never_outranks_an_earlier_field() {
    let undated = MetadataResult { year: None, ..noted_pressing("rel-undated", &[]) };
    let dated = MetadataResult {
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label One"), None)],
        ..noted_pressing("rel-dated", &[])
    };
    let outcome = noted_by_catalog(vec![undated, dated], &folder("Label One L1-100"));
    assert_eq!(offered(&outcome), vec!["rel-undated"]);
}

/// Among rows tied on everything else, a dated row is offered over an
/// undated one whose notes the folder names: the year is weighed first.
#[test]
fn a_dated_row_outranks_an_undated_one_a_note_names() {
    let undated =
        MetadataResult { year: None, ..noted_pressing("rel-undated", &["Made In Nordland"]) };
    let outcome = noted_by_catalog(
        vec![undated, noted_pressing("rel-dated", &[])],
        &folder("Label One L1-100 Nordland"),
    );
    assert_eq!(offered(&outcome), vec!["rel-dated"]);
}

/// The notes are weighed only among the rows tied on the year too: an
/// undated row, out already, does not cancel the word its notes share with
/// a dated row, so the folder naming that word offers the dated row.
#[test]
fn the_notes_are_weighed_after_the_year() {
    let undated = MetadataResult { year: None, ..noted_pressing("rel-undated", &["Plant Beta"]) };
    let outcome = noted_by_catalog(
        vec![
            noted_pressing("rel-alpha", &["Plant Alpha"]),
            noted_pressing("rel-beta", &["Plant Beta"]),
            undated,
        ],
        &folder("Label One L1-100 Beta"),
    );
    assert_eq!(offered(&outcome), vec!["rel-beta"]);
}

/// A note writing the album's artist names the album, not the pressing: the
/// folder writing the artist's name sets no row apart, though only one row's
/// notes write it.
#[test]
fn the_artist_s_name_in_a_note_keeps_the_tie() {
    let outcome = noted_by_catalog(
        vec![noted_pressing("rel-alpha", &["© 1987 Artist One."]), noted_pressing("rel-beta", &[])],
        &folder("Artist One - Album One L1-100"),
    );
    assert_eq!(offered(&outcome), vec!["rel-alpha", "rel-beta"]);
}

/// A pressing of Album One, first out in 1963, dated `year`, whose read
/// document lists ten tracks.
fn dated_pressing(release_id: &str, year: Option<i32>) -> MetadataResult {
    MetadataResult {
        title: "Album One".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist One")),
        year,
        album_first_year: Some(1963),
        source_tracks: Some(SourceTracks::Listed { count: 10 }),
        ..MetadataResult::for_test(LookupCatalog::MusicBrainz, release_id, Some("rg-album-one"))
    }
}

fn search(results: Vec<MetadataResult>, text: &CandidateText) -> Outcome {
    combine_results(
        LookupAnswers { search: results, ..LookupAnswers::default() },
        text,
        FolderAudio { track_count: 10, ..FolderAudio::UNPROVEN },
    )
}

/// A folder naming the year the album first came out names no edition: the
/// original and a reissue are offered together, neither with the Year badge,
/// and only an undated one, which tells the person less, is set aside.
#[test]
fn the_album_s_first_year_offers_every_edition() {
    let text = folder("1963 Album One");
    let results = vec![
        dated_pressing("rel-1963", Some(1963)),
        dated_pressing("rel-2005", Some(2005)),
        dated_pressing("rel-undated", None),
    ];
    let outcome = search(results.clone(), &text);
    assert_eq!(offered(&outcome), vec!["rel-1963", "rel-2005"]);
    for result in &results {
        assert!(!year_badge(result, &results, &text), "{}", result.release_id);
    }
}

/// A lone folder year, where the album's first year is not known, names no
/// pressing: most folders are named with the year the album first came out.
/// The row of that year carries no Year badge and ranks no higher than a row
/// of another year.
#[test]
fn a_lone_year_with_no_first_year_names_no_pressing() {
    let text = folder("Artist One - Album One (2005)");
    let undated_album = |release_id: &str, year: i32| {
        let mut result = dated_pressing(release_id, Some(year));
        result.album_first_year = None;
        result
    };
    let results = vec![undated_album("rel-1963", 1963), undated_album("rel-2005", 2005)];
    let outcome = search(results.clone(), &text);
    assert_eq!(offered(&outcome), vec!["rel-1963", "rel-2005"]);
    for result in &results {
        assert!(!year_badge(result, &results, &text), "{}", result.release_id);
    }
}

/// A later year names the edition: the pressing of that year is offered, and
/// where none is, an undated pressing outranks one of another year.
#[test]
fn a_later_year_picks_the_edition() {
    let remaster = folder("1963 Album One (2005 Remaster)");
    let outcome = search(
        vec![
            dated_pressing("rel-1963", Some(1963)),
            dated_pressing("rel-2005", Some(2005)),
            dated_pressing("rel-undated", None),
        ],
        &remaster,
    );
    assert_eq!(offered(&outcome), vec!["rel-2005"]);
    let outcome = search(
        vec![dated_pressing("rel-1963", Some(1963)), dated_pressing("rel-undated", None)],
        &remaster,
    );
    assert_eq!(offered(&outcome), vec!["rel-undated"]);
}

/// Whether the Year badge shows on the row of `result` among `results`.
fn year_badge(result: &MetadataResult, results: &[MetadataResult], text: &CandidateText) -> bool {
    let facts = FolderFacts::of(text, results.iter());
    agreements_of(result, text, &facts, false).year
}

/// Two folder years: the earlier is the album's, the later names the
/// pressing. Only the pressing of the later year ranks first and carries the
/// Year badge, whether or not the album's first year is known.
#[test]
fn the_later_of_two_folder_years_names_the_pressing() {
    let text = folder("Artist One - Album One 1963 (1987 Nordland Pressing)");
    for first in [Some(1963), None] {
        let dated = |release_id: &str, year: i32| {
            let mut result = dated_pressing(release_id, Some(year));
            result.album_first_year = first;
            result
        };
        let results = vec![dated("rel-1963", 1963), dated("rel-1987", 1987)];
        let outcome = search(results.clone(), &text);
        assert_eq!(offered(&outcome), vec!["rel-1987"], "first year {first:?}");
        assert!(year_badge(&results[1], &results, &text));
        assert!(!year_badge(&results[0], &results, &text));
    }
}

/// A year the artwork prints counts for nothing where the folder's name
/// writes one: the copyright year of another edition names no pressing.
#[test]
fn the_folder_s_name_outranks_a_year_the_artwork_prints() {
    let text = CandidateText::of(
        &[
            TextLine { text: "1987 Album One".to_string(), origin: TextOrigin::FolderName },
            TextLine { text: "(C) 1995 Label One".to_string(), origin: TextOrigin::Artwork },
        ],
        &[],
        &[],
    );
    let results =
        vec![dated_pressing("rel-1987", Some(1987)), dated_pressing("rel-1995", Some(1995))];
    let outcome = search(results.clone(), &text);
    assert_eq!(offered(&outcome), vec!["rel-1987"]);
    assert!(year_badge(&results[0], &results, &text));
    assert!(!year_badge(&results[1], &results, &text));
}
