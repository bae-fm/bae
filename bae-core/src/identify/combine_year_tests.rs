//! How the folder's years rank pressings of one album: a year the album first
//! came out in confirms the album, and a later one names the edition.

use super::*;
use crate::import::search::SourceTracks;
use crate::signals::{TextLine, TextOrigin};

type Outcome = (Findings, LibraryStatuses);

fn folder(line: &str) -> CandidateText {
    CandidateText::of(
        &[TextLine {
            text: line.to_string(),
            origin: TextOrigin::FolderName,
        }],
        &[],
    )
}

/// A pressing of Album One, first out in 1963, dated `year`, whose read
/// document lists ten tracks.
fn pressing(release_id: &str, year: Option<i32>) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult {
            title: "Album One".to_string(),
            artist: Some("Artist One".to_string()),
            year,
            album_first_year: Some(1963),
            source_tracks: Some(SourceTracks::Listed { count: 10 }),
            ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("rg-album-one"))
        },
        LibraryStatus::absent(release_id),
    )
}

fn search(results: Vec<(MetadataResult, LibraryStatus)>, text: &CandidateText) -> Outcome {
    combine_results(
        LookupAnswers {
            search: results,
            ..LookupAnswers::default()
        },
        Vec::new(),
        text,
        FolderAudio {
            track_count: 10,
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

/// A folder naming the year the album first came out names no edition: the
/// original, a reissue and an undated one are offered together.
#[test]
fn the_album_s_first_year_offers_every_edition() {
    let outcome = search(
        vec![
            pressing("rel-1963", Some(1963)),
            pressing("rel-2005", Some(2005)),
            pressing("rel-undated", None),
        ],
        &folder("1963 Album One"),
    );
    assert_eq!(
        offered(&outcome),
        vec!["rel-1963", "rel-2005", "rel-undated"]
    );
}

/// A later year names the edition: the pressing of that year is offered, and
/// where none is, an undated pressing outranks one of another year.
#[test]
fn a_later_year_picks_the_edition() {
    let remaster = folder("1963 Album One (2005 Remaster)");
    let outcome = search(
        vec![
            pressing("rel-1963", Some(1963)),
            pressing("rel-2005", Some(2005)),
            pressing("rel-undated", None),
        ],
        &remaster,
    );
    assert_eq!(offered(&outcome), vec!["rel-2005"]);
    let outcome = search(
        vec![
            pressing("rel-1963", Some(1963)),
            pressing("rel-undated", None),
        ],
        &remaster,
    );
    assert_eq!(offered(&outcome), vec!["rel-undated"]);
}

/// Whether the Year badge shows on the row of `result` among `results`.
fn year_badge(
    result: &MetadataResult,
    results: &[(MetadataResult, LibraryStatus)],
    text: &CandidateText,
) -> bool {
    let facts = FolderFacts::of(text, results.iter().map(|(result, _)| result));
    agreements_of(result, text, &facts, &LookupProvenance::CHOSEN).year
}

/// Two folder years: the earlier is the album's, the later names the
/// pressing. Only the pressing of the later year ranks first and carries the
/// Year badge, whether or not the album's first year is known.
#[test]
fn the_later_of_two_folder_years_names_the_pressing() {
    let text = folder("Artist One - Album One 1963 (1987 Nordland Pressing)");
    for first in [Some(1963), None] {
        let dated = |release_id: &str, year: i32| {
            let (mut result, status) = pressing(release_id, Some(year));
            result.album_first_year = first;
            (result, status)
        };
        let results = vec![dated("rel-1963", 1963), dated("rel-1987", 1987)];
        let outcome = search(results.clone(), &text);
        assert_eq!(offered(&outcome), vec!["rel-1987"], "first year {first:?}");
        assert!(year_badge(&results[1].0, &results, &text));
        assert!(!year_badge(&results[0].0, &results, &text));
    }
}

/// A year the artwork prints counts for nothing where the folder's name
/// writes one: the copyright year of another edition names no pressing.
#[test]
fn the_folder_s_name_outranks_a_year_the_artwork_prints() {
    let text = CandidateText::of(
        &[
            TextLine {
                text: "1987 Album One".to_string(),
                origin: TextOrigin::FolderName,
            },
            TextLine {
                text: "(C) 1995 Label One".to_string(),
                origin: TextOrigin::Artwork,
            },
        ],
        &[],
    );
    let results = vec![
        pressing("rel-1987", Some(1987)),
        pressing("rel-1995", Some(1995)),
    ];
    let outcome = search(results.clone(), &text);
    assert_eq!(offered(&outcome), vec!["rel-1987"]);
    assert!(year_badge(&results[0].0, &results, &text));
    assert!(!year_badge(&results[1].0, &results, &text));
}
