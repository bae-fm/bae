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
