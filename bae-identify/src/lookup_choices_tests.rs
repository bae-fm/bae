use super::{LookupChoiceEdit, LookupChoices, SearchWords};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn held() -> LookupChoices {
    LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: strings(&["9999999999999"]),
        chosen_catalogs: strings(&["LBL 001"]),
        search_words: None,
        discounted_catalogs: strings(&["LBL 100"]),
    }
}

/// Each change turns over one choice and leaves every other where it was.
#[test]
fn one_change_leaves_every_other_choice_alone() {
    let out = held().edited(LookupChoiceEdit::ToggleDiscId);
    assert!(out.disc_id_excluded);
    assert_eq!(out, LookupChoices { disc_id_excluded: true, ..held() });
    assert!(!out.edited(LookupChoiceEdit::ToggleDiscId).disc_id_excluded);

    let barcode = |code: &str| LookupChoiceEdit::ToggleBarcode { code: code.to_string() };
    let out = held().edited(barcode("0123456789012"));
    assert_eq!(out.excluded_barcodes, strings(&["0123456789012", "9999999999999"]));
    assert_eq!(out.chosen_catalogs, held().chosen_catalogs);
    assert!(held().edited(barcode("9999999999999")).excluded_barcodes.is_empty());
}

/// Chosen numbers are looked up in the order they were chosen, and a
/// number chosen again is taken out.
#[test]
fn a_catalog_number_joins_and_leaves_in_order() {
    let catalog = |number: &str| LookupChoiceEdit::ToggleCatalog { number: number.to_string() };
    let added = held().edited(catalog("LBL 002"));
    assert_eq!(added.chosen_catalogs, strings(&["LBL 001", "LBL 002"]));
    assert_eq!(added.edited(catalog("LBL 001")).chosen_catalogs, strings(&["LBL 002"]));
}

/// Striking a number out chooses nothing and unchooses nothing: a chosen
/// number stays chosen, and counting a number again chooses nothing
/// either.
#[test]
fn striking_a_number_out_or_back_chooses_nothing() {
    let discounted =
        |number: &str| LookupChoiceEdit::ToggleDiscounted { number: number.to_string() };
    let struck = held().edited(discounted("LBL 001"));
    assert_eq!(struck.discounted_catalogs, strings(&["LBL 001", "LBL 100"]));
    assert_eq!(struck.chosen_catalogs, strings(&["LBL 001"]));
    let counted = held().edited(discounted("LBL 100"));
    assert!(counted.discounted_catalogs.is_empty());
    assert_eq!(counted.chosen_catalogs, strings(&["LBL 001"]));
}

/// Typed words replace the draft's title, and blank ones give it back.
#[test]
fn search_words_replace_the_title_and_blank_ones_give_it_back() {
    let typed = held().edited(LookupChoiceEdit::SearchBy {
        album: " Album Title ".to_string(),
        artist: "Artist".to_string(),
    });
    assert_eq!(
        typed.search_words,
        Some(SearchWords { album: "Album Title".to_string(), artist: "Artist".to_string() })
    );
    let cleared =
        typed.edited(LookupChoiceEdit::SearchBy { album: String::new(), artist: String::new() });
    assert_eq!(cleared.search_words, None);
}

/// Striking a number out and choosing it are separate choices: a number
/// in both lists stays in both, whichever way the two lists spell it.
#[test]
fn a_struck_out_number_stays_chosen() {
    let normalized = LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: Vec::new(),
        chosen_catalogs: strings(&["WPCR-80001", "NJ 8255", "COCQ 84487"]),
        search_words: None,
        discounted_catalogs: strings(&["nj-8255"]),
    }
    .normalized();
    assert_eq!(normalized.chosen_catalogs, strings(&["WPCR-80001", "NJ 8255", "COCQ 84487"]));
    assert_eq!(normalized.discounted_catalogs, strings(&["nj-8255"]));
}

/// One number is chosen once: the second spelling of it is dropped and
/// the first stands where it was chosen.
#[test]
fn a_number_is_chosen_once_however_it_is_spelled() {
    let normalized = LookupChoices {
        chosen_catalogs: strings(&["NJ-8255", "WPCR-80001", "NJ 8255"]),
        ..LookupChoices::default()
    }
    .normalized();
    assert_eq!(normalized.chosen_catalogs, strings(&["NJ-8255", "WPCR-80001"]));
}

/// Typed words keep their letters and lose their edges; words naming no
/// title are no words, and the draft's title is searched.
#[test]
fn search_words_are_trimmed_and_blank_ones_are_none() {
    let typed = LookupChoices {
        search_words: Some(SearchWords {
            album: "  Album Title  ".to_string(),
            artist: " Artist ".to_string(),
        }),
        ..LookupChoices::default()
    }
    .normalized();
    assert_eq!(
        typed.search_words,
        Some(SearchWords { album: "Album Title".to_string(), artist: "Artist".to_string() })
    );
    let blank = LookupChoices {
        search_words: Some(SearchWords { album: "   ".to_string(), artist: "Artist".to_string() }),
        ..LookupChoices::default()
    }
    .normalized();
    assert_eq!(blank.search_words, None);
}

/// Different words are a different question for the providers.
#[test]
fn different_search_words_ask_something_else() {
    let draft = LookupChoices::default();
    let typed = LookupChoices {
        search_words: Some(SearchWords { album: "Album Title".to_string(), artist: String::new() }),
        ..LookupChoices::default()
    };
    assert!(!draft.asks_the_same_as(&typed));
    assert!(typed.asks_the_same_as(&typed.clone()));
}
