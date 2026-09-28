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
/// stating another disagrees, an undated one states nothing. Of two folder
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

/// With no album year known, no folder year is known to name an edition.
#[test]
fn with_no_album_year_no_year_names_an_edition() {
    let facts = FolderFacts::of(&text(&["1963 Album"]), []);
    for year in [Some(1963), Some(2005), None] {
        assert_eq!(
            facts.edition_year(&[record(year, None, None)]),
            Fact::StatesNothing,
            "{year:?}"
        );
    }
}

/// A country the folder names agrees; another country disagrees; a region, or
/// no area, states nothing; and a folder naming no country contradicts none.
#[test]
fn a_country_agrees_disagrees_or_states_nothing() {
    let named = text(&["Album (Italy)"]);
    let facts = FolderFacts::of(&named, []);
    let country = |area| facts.country(&[record(None, None, area)], &named);
    assert_eq!(country(Some("IT")), Fact::Agrees);
    assert_eq!(country(Some("US")), Fact::Disagrees);
    assert_eq!(country(Some("XE")), Fact::StatesNothing);
    assert_eq!(country(None), Fact::StatesNothing);
    let unnamed = text(&["Album CD"]);
    let facts = FolderFacts::of(&unnamed, []);
    assert_eq!(
        facts.country(&[record(None, None, Some("US"))], &unnamed),
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
