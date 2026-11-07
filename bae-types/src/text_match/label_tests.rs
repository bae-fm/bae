use super::*;

/// Every entry is written with words: one that is not would be dropped
/// from the table, and the trade word it means to name would stay on
/// every label's name.
#[test]
fn every_trade_word_is_written_with_words() {
    for entry in TRADE_WORDS {
        assert!(!crate::text_match::words(entry).is_empty(), "{entry} is written with no words");
    }
    assert_eq!(tails().len(), TRADE_WORDS.len());
}

fn stated(name: &str) -> Option<String> {
    LabelName::of(name).map(|name| name.stated().to_string())
}

/// A name is left with what names the label, however many trade words it
/// trails and however they are punctuated.
#[test]
fn a_name_keeps_only_what_names_the_label() {
    assert_eq!(stated("Label Name Bros. Records").as_deref(), Some("labelnamebros"));
    assert_eq!(stated("Label Name Music Entertainment").as_deref(), Some("labelname"));
    assert_eq!(stated("Label Name Record Co.").as_deref(), Some("labelname"));
    assert_eq!(stated("Label Name Tune").as_deref(), Some("labelnametune"));
}

/// Trade words alone name no label.
#[test]
fn a_name_of_trade_words_alone_names_nothing() {
    assert_eq!(stated("Records"), None);
    assert_eq!(stated("Music Entertainment"), None);
    assert_eq!(stated(""), None);
    assert_eq!(stated("···"), None);
}

/// Two names name one label when they read alike without their trade
/// words, or when one is written as the other's initials.
#[test]
fn two_names_name_one_label() {
    let same = |a: &str, b: &str| LabelName::of(a).unwrap().same_label(&LabelName::of(b).unwrap());
    assert!(same("Harbor Records", "Harbor"));
    assert!(same("ABC", "Alpha Beta Corporation"));
    assert!(same("Alpha Beta Corporation Records", "A.B.C."));
    assert!(!same("abc", "Alpha Beta Corporation"));
    assert!(!same("ABD", "Alpha Beta Corporation"));
    assert!(!same("Harbor", "Summit"));
}

/// A label's key drops its trade words and runs its words together, so
/// spellings with and without them are one key; initials are a key of
/// their own, and trade words alone are no key.
#[test]
fn a_label_key_is_its_words_without_trade_words() {
    assert_eq!(label_name_key("Label Name Records"), "labelname");
    assert_eq!(label_name_key("LABEL NAME"), "labelname");
    assert_eq!(label_name_key("Label Name Bros. Records Inc."), "labelnamebros");
    assert_eq!(label_name_key("DFC"), "dfc");
    assert_ne!(label_name_key("DFC"), label_name_key("Dance Floor Corporation"));
    assert_eq!(label_name_key("Records"), "");
}

/// Written names compare as their labels do; names of trade words alone
/// compare as written.
#[test]
fn two_written_names_name_one_label() {
    assert!(same_label_name("Harbor Records", "harbor"));
    assert!(same_label_name("A.B.C.", "Alpha Beta Corporation Ltd"));
    assert!(same_label_name("Records", "records"));
    assert!(!same_label_name("Records", "Recordings"));
    assert!(!same_label_name("Records", "Harbor Records"));
}
