use super::*;
use crate::import::Catalog;
use crate::signals::{TextLine, TextOrigin};

fn text(lines: &[&str]) -> CandidateText {
    CandidateText::of(
        &lines
            .iter()
            .map(|line| TextLine {
                text: (*line).to_string(),
                origin: TextOrigin::FolderName,
            })
            .collect::<Vec<_>>(),
        &[],
        &[],
    )
}

/// A record titled "Album", dated `year`, of an album first out in `first`,
/// released in `area`.
fn record(year: Option<i32>, first: Option<i32>, area: Option<&str>) -> MetadataResult {
    MetadataResult {
        year,
        album_first_year: first,
        area: area.map(crate::pressing::area),
        ..MetadataResult::for_test(Catalog::MusicBrainz, "rel-1", Some("rg-1"))
    }
}

/// A folder year equal to the album's first year confirms the album and names
/// no edition: every row, dated or not, states nothing about the edition.
#[test]
fn the_album_s_own_year_confirms_the_album_and_names_no_edition() {
    let facts = FolderFacts::of(&text(&["1963 Album"]), []);
    for year in [Some(1963), Some(2005), None] {
        let rows = [record(year, Some(1963), None)];
        assert!(facts.names_the_album_year(&rows));
        assert_eq!(facts.edition_year(&rows), Fact::StatesNothing, "{year:?}");
    }
}

/// A later folder year names the edition: the row stating it agrees, one
/// stating another disagrees, an unplaced one states nothing. Of two folder
/// years, the later is the edition's.
#[test]
fn a_later_folder_year_names_the_edition() {
    let facts = FolderFacts::of(&text(&["1963 Album", "2005 Remaster"]), []);
    let edition = |year| facts.edition_year(&[record(year, Some(1963), None)]);
    assert_eq!(edition(Some(2005)), Fact::Agrees);
    assert_eq!(edition(Some(1963)), Fact::Disagrees);
    assert_eq!(edition(None), Fact::StatesNothing);
    assert!(facts.names_the_album_year(&[record(Some(2005), Some(1963), None)]));
}

/// A year the row's own title writes is the title's, not an edition's.
#[test]
fn a_year_the_title_writes_names_no_edition() {
    let facts = FolderFacts::of(&text(&["1990-2000 Album 1991"]), []);
    let mut titled = record(Some(1991), Some(1991), None);
    titled.title = "Album 1990-2000".to_string();
    assert_eq!(facts.edition_year(&[titled]), Fact::StatesNothing);
}

/// With no album year known, a lone folder year names no edition: it is as
/// often the album's.
#[test]
fn with_no_album_year_a_lone_year_names_no_edition() {
    let facts = FolderFacts::of(&text(&["1963 Album"]), []);
    for year in [Some(1963), Some(2005), None] {
        let rows = [record(year, None, None)];
        assert_eq!(facts.pressing_year(&rows), None, "{year:?}");
        assert_eq!(facts.edition_year(&rows), Fact::StatesNothing, "{year:?}");
    }
}

/// With no album year known, of two folder years the later names the
/// edition.
#[test]
fn with_no_album_year_the_later_of_two_years_names_the_edition() {
    let facts = FolderFacts::of(&text(&["1963 Album", "2005 Remaster"]), []);
    let edition = |year| facts.edition_year(&[record(year, None, None)]);
    assert_eq!(facts.pressing_year(&[record(None, None, None)]), Some(2005));
    assert_eq!(edition(Some(2005)), Fact::Agrees);
    assert_eq!(edition(Some(1963)), Fact::Disagrees);
    assert_eq!(edition(None), Fact::StatesNothing);
}

/// A country the folder names agrees; another country disagrees; a region, or
/// no area, states nothing; and a folder naming no country contradicts none.
#[test]
fn a_country_agrees_disagrees_or_states_nothing() {
    let named = text(&["Album (Italy)"]);
    let facts = FolderFacts::of(&named, []);
    let country = |area| facts.country(&[record(None, None, area)]);
    assert_eq!(country(Some("IT")), Fact::Agrees);
    assert_eq!(country(Some("US")), Fact::Disagrees);
    assert_eq!(country(Some("XE")), Fact::StatesNothing);
    assert_eq!(country(None), Fact::StatesNothing);
    let unnamed = text(&["Album CD"]);
    let facts = FolderFacts::of(&unnamed, []);
    assert_eq!(
        facts.country(&[record(None, None, Some("US"))]),
        Fact::StatesNothing
    );
}

/// The registration country agrees with a row released there, disagrees with
/// one released in another country, and says nothing of a region or no area.
#[test]
fn the_registration_country_agrees_disagrees_or_states_nothing() {
    let italy = Some(crate::pressing::area("IT"));
    let registered = |area| registration(&[record(None, None, area)], italy);
    assert_eq!(registered(Some("IT")), Fact::Agrees);
    assert_eq!(registered(Some("US")), Fact::Disagrees);
    assert_eq!(registered(Some("XE")), Fact::StatesNothing);
    assert_eq!(registered(None), Fact::StatesNothing);
    assert_eq!(
        registration(&[record(None, None, Some("US"))], None),
        Fact::StatesNothing
    );
}

/// Agreeing outranks stating nothing, which outranks disagreeing.
#[test]
fn a_fact_ranks_agrees_above_nothing_above_disagrees() {
    assert!(Fact::Agrees > Fact::StatesNothing);
    assert!(Fact::StatesNothing > Fact::Disagrees);
}

/// Every pressing of one album shares the year it first came out, read off
/// any one of them: an unread pressing of an album a read one dates is dated
/// too.
#[test]
fn an_album_s_first_year_is_every_pressing_s() {
    let read = record(Some(1963), Some(1963), None);
    let unread = record(Some(2005), None, None);
    let facts = FolderFacts::of(&text(&["1963 Album"]), [&read, &unread]);
    assert_eq!(
        facts.album_first_year(std::slice::from_ref(&unread)),
        Some(1963)
    );
    assert!(facts.names_the_album_year(std::slice::from_ref(&unread)));
}

/// A record whose read document lists `titles`.
fn listing(titles: &[&str]) -> MetadataResult {
    MetadataResult {
        track_titles: titles.iter().map(|title| title.to_string()).collect(),
        ..MetadataResult::for_test(Catalog::MusicBrainz, "rel-1", Some("rg-1"))
    }
}

fn titles(titles: &[&str]) -> Vec<String> {
    titles.iter().map(|title| title.to_string()).collect()
}

/// Every position matching, loosely, agrees: case, accents and a bracketed
/// version aside, and a file named "Artist - Title".
#[test]
fn titles_in_the_folder_s_order_agree() {
    let folder = titles(&["Artist Name - Song One", "Chanson Deux", "Song Three"]);
    let listed = listing(&["Song One (Remastered)", "chanson deux", "Song Three [Live]"]);
    assert_eq!(track_titles(&[listed], &folder), Fact::Agrees);
}

/// The same titles, every one, at other positions disagree.
#[test]
fn the_same_titles_in_another_order_disagree() {
    let folder = titles(&["Song One", "Song Two", "Song Three"]);
    let listed = listing(&["Song Two", "Song One", "Song Three"]);
    assert_eq!(track_titles(&[listed], &folder), Fact::Disagrees);
}

/// A spelling, a language, a missing title, another count, and a document not
/// read state nothing — never a disagreement.
#[test]
fn titles_that_differ_otherwise_state_nothing() {
    let folder = titles(&["Song One", "Song Two", "Song Three"]);
    for listed in [
        listing(&["Song Two", "Song Won", "Song Three"]),
        listing(&["Chanson Deux", "Chanson Un", "Song Three"]),
        listing(&["Song Two", "Song One"]),
        listing(&["Song Two", "Song One", "Song Three", "Song Four"]),
        listing(&["Song Two", "(Untitled)", "Song Three"]),
        listing(&[]),
    ] {
        assert_eq!(
            track_titles(std::slice::from_ref(&listed), &folder),
            Fact::StatesNothing,
            "{:?}",
            listed.track_titles
        );
    }
    let listed = listing(&["Song Two", "Song One", "Song Three"]);
    assert_eq!(track_titles(&[listed], &[]), Fact::StatesNothing);
}

/// Of a row's records, one listing the titles in order decides it.
#[test]
fn a_record_in_order_decides_its_row() {
    let folder = titles(&["Song One", "Song Two"]);
    let rows = [
        listing(&["Song Two", "Song One"]),
        listing(&["Song One", "Song Two"]),
    ];
    assert_eq!(track_titles(&rows, &folder), Fact::Agrees);
}

/// Lines read off `origin`s: the folder's own text and what else it carries.
fn read(lines: &[(TextOrigin, &str)]) -> CandidateText {
    CandidateText::of(
        &lines
            .iter()
            .map(|(origin, text)| TextLine {
                text: (*text).to_string(),
                origin: *origin,
            })
            .collect::<Vec<_>>(),
        &[],
        &[],
    )
}

/// A row of `area`, its own record.
fn released_in(release_id: &str, area: &str) -> MetadataResult {
    MetadataResult {
        area: Some(crate::pressing::area(area)),
        ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("rg-1"))
    }
}

/// The folder's name outranks the artwork: a country the name gives is the
/// folder's, and a country only a scanned lyric writes counts for nothing.
#[test]
fn the_folder_s_name_outranks_the_artwork_s_country() {
    let canada = released_in("rel-ca", "CA");
    let us = released_in("rel-us", "US");
    let facts = FolderFacts::of(
        &read(&[
            (
                TextOrigin::FolderName,
                "Artist - Album [Label CAT-1, Canada]",
            ),
            (TextOrigin::Artwork, "Freedom is coming to the U.S.A."),
        ]),
        [&canada, &us],
    );
    assert_eq!(facts.country(std::slice::from_ref(&canada)), Fact::Agrees);
    assert_eq!(facts.country(std::slice::from_ref(&us)), Fact::Disagrees);
    assert!(facts.names_area(crate::pressing::area("CA")));
    assert!(!facts.names_area(crate::pressing::area("US")));
}

/// Two countries in the text of the highest standing leave the folder with
/// none: no row gets the point.
#[test]
fn two_countries_in_the_folder_s_name_name_none() {
    let canada = released_in("rel-ca", "CA");
    let us = released_in("rel-us", "US");
    let facts = FolderFacts::of(
        &read(&[(TextOrigin::FolderName, "Album (Canada, USA)")]),
        [&canada, &us],
    );
    for row in [&canada, &us] {
        assert_eq!(
            facts.country(std::slice::from_ref(row)),
            Fact::StatesNothing
        );
    }
    assert!(!facts.names_area(crate::pressing::area("CA")));
    assert!(!facts.names_area(crate::pressing::area("US")));
}

/// Where only the artwork names a country, that country is the folder's.
#[test]
fn a_country_only_the_artwork_names_counts() {
    let canada = released_in("rel-ca", "CA");
    let us = released_in("rel-us", "US");
    let facts = FolderFacts::of(
        &read(&[
            (TextOrigin::FolderName, "Artist - Album"),
            (TextOrigin::Artwork, "Made in Canada"),
        ]),
        [&canada, &us],
    );
    assert_eq!(facts.country(std::slice::from_ref(&canada)), Fact::Agrees);
    assert_eq!(facts.country(std::slice::from_ref(&us)), Fact::Disagrees);
}

/// A country no row was released in is still the folder's: every row of a
/// country disagrees, and a row stating none states nothing.
#[test]
fn a_country_no_row_has_disagrees_with_every_row() {
    let canada = released_in("rel-ca", "CA");
    let us = released_in("rel-us", "US");
    let unplaced = MetadataResult::for_test(Catalog::MusicBrainz, "rel-none", Some("rg-1"));
    let facts = FolderFacts::of(
        &read(&[(TextOrigin::FolderName, "Album (Japan)")]),
        [&canada, &us, &unplaced],
    );
    assert_eq!(
        facts.country(std::slice::from_ref(&canada)),
        Fact::Disagrees
    );
    assert_eq!(facts.country(std::slice::from_ref(&us)), Fact::Disagrees);
    assert_eq!(
        facts.country(std::slice::from_ref(&unplaced)),
        Fact::StatesNothing
    );
}

/// A row carrying `numbers` as its catalog numbers.
fn numbered(release_id: &str, numbers: &[&str]) -> MetadataResult {
    MetadataResult {
        labels: numbers
            .iter()
            .map(|number| crate::pressing::ReleaseLabel::of(Some("Label One"), Some(number)))
            .collect(),
        ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("rg-1"))
    }
}

/// The folder's name outranks the artwork: a number only the artwork prints
/// states nothing where the name states one.
#[test]
fn the_folder_s_name_outranks_the_artwork_s_catalog_number() {
    let named = numbered("rel-a", &["LBL-100"]);
    let printed = numbered("rel-b", &["LBL-200"]);
    let facts = FolderFacts::of(
        &read(&[
            (
                TextOrigin::FolderName,
                "Artist - Album (Label One, LBL-100)",
            ),
            (TextOrigin::Artwork, "LBL-200"),
        ]),
        [&named, &printed],
    );
    assert!(facts.states_catalog("LBL-100"));
    assert!(!facts.states_catalog("LBL-200"));
}

/// The digits beneath the folder's barcode are the barcode printed again: a
/// number spelling them states nothing, even where only the artwork speaks.
#[test]
fn a_barcode_s_digits_state_no_catalog_number() {
    let named = numbered("rel-a", &["LBL-100"]);
    let barcoded = numbered("rel-b", &["1234-56789-0"]);
    let text = CandidateText::of(
        &[TextLine {
            text: "LBL-100 0 1234-56789-0 5".to_string(),
            origin: TextOrigin::Artwork,
        }],
        &[],
        &[crate::signals::SourcedValue::new(
            "012345678905".to_string(),
        )],
    );
    let facts = FolderFacts::of(&text, [&named, &barcoded]);
    assert!(facts.states_catalog("LBL-100"));
    assert!(!facts.states_catalog("1234-56789-0"));
}

/// A copy can state several numbers: a row carrying either agrees.
#[test]
fn every_number_the_folder_states_counts() {
    let first = numbered("rel-a", &["LBL-100"]);
    let second = numbered("rel-b", &["ALT-300"]);
    let facts = FolderFacts::of(
        &read(&[(TextOrigin::FolderName, "Album (LBL-100 / ALT-300)")]),
        [&first, &second],
    );
    assert!(facts.states_catalog("LBL-100"));
    assert!(facts.states_catalog("ALT-300"));
}

/// A row's number read out of a longer one the folder's name prints is no
/// number the name states: the name states nothing, and the artwork, which
/// prints the row's number whole, speaks.
#[test]
fn a_piece_of_a_longer_number_is_no_number_the_folder_states() {
    let short = numbered("rel-a", &["AB12"]);
    let named = FolderFacts::of(
        &read(&[(TextOrigin::FolderName, "Artist - Album [AB12-2]")]),
        [&short],
    );
    assert!(!named.states_catalog("AB12"));
    let printed = FolderFacts::of(
        &read(&[
            (TextOrigin::FolderName, "Artist - Album [AB12-2]"),
            (TextOrigin::Artwork, "AB12"),
        ]),
        [&short],
    );
    assert!(printed.states_catalog("AB12"));
}

/// A number the folder's name writes that no row carries does not silence
/// the artwork: the row whose number the artwork prints agrees.
#[test]
fn a_number_no_row_carries_leaves_the_next_standing_to_speak() {
    let first = numbered("rel-a", &["LBL-100", "12-90329-2"]);
    let second = numbered("rel-b", &["LBL-200"]);
    let facts = FolderFacts::of(
        &read(&[
            (TextOrigin::FolderName, "Artist - Album (Label One 90329-2)"),
            (TextOrigin::Artwork, "12-90329-2 YS"),
        ]),
        [&first, &second],
    );
    assert!(facts.states_catalog("12-90329-2"));
    assert!(!facts.states_catalog("LBL-200"));
}

/// A pick's year: the year a record of the row states — another catalog's
/// record of the pressing speaks for the one the draft is read from — and,
/// where none states one, the year the folder names the pressing by.
#[test]
fn a_pick_s_year_is_a_record_s_else_the_folder_s() {
    let folder = text(&["1986 Germany Label AB 12345-2", "1979 Album"]);
    let undated = record(None, Some(1979), None);
    assert_eq!(
        pressing_year(&folder, std::slice::from_ref(&undated)),
        Some(1986)
    );
    let partner = MetadataResult {
        year: Some(1987),
        ..MetadataResult::for_test(Catalog::Discogs, "dg-1", Some("m-1"))
    };
    assert_eq!(
        pressing_year(&folder, &[undated.clone(), partner]),
        Some(1987)
    );
    assert_eq!(
        pressing_year(&folder, &[record(Some(1990), Some(1979), None)]),
        Some(1990)
    );
    assert_eq!(pressing_year(&text(&["Album"]), &[undated]), None);
}
