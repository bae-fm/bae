use super::{calls_of, Call};

#[test]
fn each_fragment_and_read_call_gives_its_name_line_and_sql() {
    let calls = calls_of(
        r##"
sqlive_compose::fragment!(Person { person_id: PersonId }, r#"
    SELECT p.id AS person_id FROM people AS p
"#);

mod inner {
    sqlive_compose::read!(Page(issue: Option<IssueId>), r#"SELECT 1 FROM issues"#);
}
"##,
    )
    .unwrap();
    assert_eq!(
        calls,
        [
            Call {
                macro_name: "fragment".into(),
                name: "Person".into(),
                line: 2,
                sql: "\n    SELECT p.id AS person_id FROM people AS p\n".into(),
            },
            Call {
                macro_name: "read".into(),
                name: "Page".into(),
                line: 7,
                sql: "SELECT 1 FROM issues".into(),
            },
        ]
    );
}

#[test]
fn another_macro_and_a_call_of_the_wrong_shape_are_not_calls() {
    let calls = calls_of("fn f() { println!(\"x\"); vec![1]; sqlive_compose::fragment!(1); fragment!(Other, \"SELECT 1\"); }").unwrap();
    assert_eq!(calls, []);
}

#[test]
fn source_that_does_not_parse_is_an_error() {
    assert!(calls_of("fn (").is_err());
}
