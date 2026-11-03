use super::*;

#[test]
fn blank_and_whitespace_are_rejected() {
    assert_eq!(LibraryName::parse(""), Err(EmptyLibraryName));
    assert_eq!(LibraryName::parse("   "), Err(EmptyLibraryName));
    assert_eq!(LibraryName::parse("\t\n"), Err(EmptyLibraryName));
}

#[test]
fn a_name_is_trimmed() {
    let name = LibraryName::parse("  Jazz  ").expect("non-blank");
    assert_eq!(name.as_str(), "Jazz");
}

#[test]
fn generated_names_are_non_blank() {
    assert!(!generate_library_name().as_str().is_empty());
}
