use super::*;
use sha2::{Digest, Sha256};

fn sorted(language: &str, names: &[&str]) -> Vec<String> {
    let keys = NameKeys::for_language(language).unwrap();
    let mut names: Vec<String> = names.iter().map(ToString::to_string).collect();
    names.sort_by_key(|name| keys.key(name));
    names
}

#[test]
fn the_root_order_puts_accented_letters_with_their_base_and_ignores_no_case() {
    assert_eq!(
        sorted("und", &["Zed", "Émile", "Eve", "zed", "Ötzi", "Oz"]),
        ["Émile", "Eve", "Ötzi", "Oz", "zed", "Zed"]
    );
    assert_ne!(
        NameKeys::for_language("und").unwrap().key("a"),
        NameKeys::for_language("und").unwrap().key("A"),
        "names that differ only in case have keys that do"
    );
}

#[test]
fn a_language_sorts_by_its_own_alphabet() {
    // Swedish puts å, ä and ö after z; the root order puts them with a and o.
    assert_eq!(
        sorted("sv", &["Öl", "Zebra", "Åke", "Olle", "Ärlig", "Anna"]),
        ["Anna", "Olle", "Zebra", "Åke", "Ärlig", "Öl"]
    );
    assert_eq!(
        sorted("und", &["Öl", "Zebra", "Åke", "Olle", "Ärlig", "Anna"]),
        ["Åke", "Anna", "Ärlig", "Öl", "Olle", "Zebra"]
    );
    // Spanish sorts ñ after n.
    assert_eq!(sorted("es", &["Ñu", "Oso", "Nube", "Zorro"]), ["Nube", "Ñu", "Oso", "Zorro"]);
}

#[test]
fn cyrillic_letters_sort_in_the_alphabet_with_both_cases_together() {
    assert_eq!(
        sorted("ru", &["Яблоко", "Арбуз", "яма", "Борщ"]),
        ["Арбуз", "Борщ", "Яблоко", "яма"]
    );
}

#[test]
fn a_list_of_names_comes_before_the_lists_it_starts() {
    let keys = NameKeys::for_language("und").unwrap();
    let list = |names: &[&str]| keys.list_key(&names.join(&LIST_SEPARATOR.to_string()));
    let mut lists = vec![list(&["A", "C"]), list(&["A", "B"]), list(&["A"]), list(&["AA"])];
    lists.sort();
    assert_eq!(lists, [list(&["A"]), list(&["A", "B"]), list(&["A", "C"]), list(&["AA"])]);
}

#[test]
fn a_language_tag_is_checked() {
    let refused = NameKeys::for_language("not a tag").err().unwrap();
    assert_eq!(refused.tag, "not a tag");
    assert!(std::error::Error::source(&refused).is_some(), "the parse error is kept");
    assert_eq!(NameKeys::for_language("sv-SE").unwrap().tag(), "sv-SE");
}

/// A key made by new collation data differs from the stored ones, which are
/// then wrong: this fails when the keys of a fixed corpus change, and the fix
/// is to bump `DATA_VERSION` and update the digest below, which makes every
/// device rewrite its stored keys.
#[test]
fn keys_are_the_version_they_claim() {
    let corpus = [
        "Abba",
        "abba",
        "Émile",
        "Ötzi",
        "Åke",
        "Ñu",
        "Яблоко",
        "ça va",
        "東京",
        "ひらがな",
        "서울",
        "ß",
        "",
    ];
    let mut digest = Sha256::new();
    for language in ["und", "sv", "es", "ru", "ja", "ko", "zh-Hans", "tr", "da"] {
        let keys = NameKeys::for_language(language).unwrap();
        for name in corpus {
            digest.update(keys.key(name));
            digest.update([0xff]);
        }
    }
    assert_eq!(
        (DATA_VERSION, hex::encode(digest.finalize())),
        (1, "5388ba8c453cf748f7cd3a1f2ae2350e721fa68ed4fd4a72a77619557ea1462d".to_string()),
    );
}
