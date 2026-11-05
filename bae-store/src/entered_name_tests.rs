use super::*;

#[test]
fn a_blank_name_is_refused() {
    assert_eq!(EnteredName::parse(""), Err(BlankName));
    assert_eq!(EnteredName::parse("   "), Err(BlankName));
    assert_eq!(EnteredName::parse("\t\n"), Err(BlankName));
}

#[test]
fn a_name_is_trimmed() {
    assert_eq!(EnteredName::parse("  Jazz  ").unwrap().as_str(), "Jazz");
}
