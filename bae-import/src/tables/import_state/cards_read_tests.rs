//! How a row's stored statements read back into its facts.

use super::*;

fn label_name(line: usize, value: &str) -> StoredStatement {
    StoredStatement {
        fact: "label_name".to_string(),
        line,
        value: value.to_string(),
        catalogs: Vec::new(),
    }
}

/// The label lines the layout write numbered from zero read back in order.
#[test]
fn label_lines_read_back_in_their_order() {
    let statements =
        statements_of(vec![label_name(0, "First Imprint"), label_name(1, "Second Imprint")])
            .unwrap();
    let names: Vec<&[String]> =
        statements.labels.iter().map(|line| line.names.as_slice()).collect();
    assert_eq!(names, [["First Imprint".to_string()], ["Second Imprint".to_string()]]);
}

/// A label line past the next one is damaged data, never a blank line.
#[test]
fn a_gap_in_the_label_lines_is_unreadable() {
    let read = statements_of(vec![label_name(0, "First Imprint"), label_name(2, "Third Imprint")]);
    assert!(matches!(read, Err(DbError::Unreadable { .. })), "{read:?}");
}
