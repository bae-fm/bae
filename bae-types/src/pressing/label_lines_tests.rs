use super::*;

fn label(name: Option<&str>, number: Option<&str>) -> ReleaseLabel {
    ReleaseLabel::of(name, number)
}

fn line(names: &[&str], numbers: &[&str]) -> LabelLine {
    LabelLine {
        names: names.iter().map(|name| name.to_string()).collect(),
        catalog_numbers: numbers.iter().map(|number| number.to_string()).collect(),
    }
}

/// A name stated with several numbers shows once, with every number, at
/// the place the name first appears.
#[test]
fn a_name_is_shown_once_with_every_number_under_it() {
    let labels = [
        label(Some("Label One"), Some("AB-100")),
        label(Some("Label Two"), Some("CD-200")),
        label(Some("label one"), Some("EF-300")),
    ];
    assert_eq!(
        label_lines(&labels),
        vec![line(&["Label One"], &["AB-100", "EF-300"]), line(&["Label Two"], &["CD-200"]),]
    );
}

/// Names stated with the same number show together, the number once, at
/// the place the first of them appears.
#[test]
fn names_sharing_a_number_show_it_once() {
    let labels = [
        label(Some("Label One"), Some("AB-100")),
        label(Some("Label Two"), Some("AB 100")),
        label(Some("Label Three"), Some("CD-200")),
        label(Some("Label Four"), Some("AB-100")),
    ];
    assert_eq!(
        label_lines(&labels),
        vec![
            line(&["Label One", "Label Two", "Label Four"], &["AB-100"]),
            line(&["Label Three"], &["CD-200"]),
        ]
    );
}

/// Two names of one label — one trailing a trade word, one written as
/// the other's initials — are the one label, shown by the name it first
/// appears as.
#[test]
fn names_of_one_label_show_once() {
    let labels = [
        label(Some("Harbor Records"), Some("AB-100")),
        label(Some("Harbor"), Some("CD-200")),
        label(Some("Alpha Beta Corporation"), Some("EF-300")),
        label(Some("A.B.C."), Some("GH-400")),
    ];
    assert_eq!(
        label_lines(&labels),
        vec![
            line(&["Harbor Records"], &["AB-100", "CD-200"]),
            line(&["Alpha Beta Corporation"], &["EF-300", "GH-400"]),
        ]
    );
}

/// Names only group when every number is the same; a number with no name
/// and a name with no number each stand alone.
#[test]
fn only_the_same_numbers_group_names() {
    let labels = [
        label(Some("Label One"), Some("AB-100")),
        label(Some("Label One"), Some("CD-200")),
        label(Some("Label Two"), Some("AB-100")),
        label(None, Some("EF-300")),
        label(Some("Label Three"), None),
        label(Some("Label Four"), None),
    ];
    assert_eq!(
        label_lines(&labels),
        vec![
            line(&["Label One"], &["AB-100", "CD-200"]),
            line(&["Label Two"], &["AB-100"]),
            line(&[], &["EF-300"]),
            line(&["Label Three"], &[]),
            line(&["Label Four"], &[]),
        ]
    );
}
