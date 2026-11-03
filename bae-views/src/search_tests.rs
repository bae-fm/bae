use super::*;

#[test]
fn blank_and_whitespace_are_not_searches() {
    assert_eq!(LibrarySearchQuery::parse(""), None);
    assert_eq!(LibrarySearchQuery::parse("   "), None);
    assert_eq!(LibrarySearchQuery::parse("\t \n"), None);
}

#[test]
fn a_query_is_trimmed() {
    let query = LibrarySearchQuery::parse("  Album Title  ").expect("non-blank");
    assert_eq!(query.as_str(), "Album Title");
}

#[test]
fn interior_whitespace_is_preserved() {
    let query = LibrarySearchQuery::parse("dark side").expect("non-blank");
    assert_eq!(query.as_str(), "dark side");
}
