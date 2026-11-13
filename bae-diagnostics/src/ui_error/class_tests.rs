use super::*;

/// Each class goes by its own name, and the name finds it again: a name
/// two classes shared would read one back as the other.
#[test]
fn every_class_is_named_back_by_its_own_name() {
    for class in UiErrorClass::ALL {
        assert_eq!(UiErrorClass::named(class.name()), Some(class), "{class:?}");
    }
}

/// A class that carries nothing more is its category's class, and a name no
/// class goes by names none.
#[test]
fn a_plain_class_reads_back_as_its_category() {
    for class in UiErrorClass::ALL {
        if let Some(category) = UiErrorCategory::plain(class) {
            assert_eq!(category.class(), class);
        }
    }
    assert_eq!(UiErrorClass::named("no_such_class"), None);
}

/// A class whose line names the whole fault shows the line alone: its detail
/// is the same sentence again, in English. A class whose detail carries a
/// cause the line leaves out keeps it, under the line.
#[test]
fn a_detail_shows_only_when_it_says_more_than_the_line() {
    // "this device already imported these files, as release …" under
    // "This device already imported these files, so they are not imported
    // again."
    assert!(!UiErrorClass::FilesAlreadyImported.detail_adds_to_line());
    assert!(!UiErrorClass::AlbumGone.detail_adds_to_line());
    // "dns error: no such host" under "Couldn't reach storage. Check your
    // connection and try again."
    assert!(UiErrorClass::Network.detail_adds_to_line());
}
