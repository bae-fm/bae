use super::*;

#[test]
fn plain_text() {
    assert_eq!(
        parse("that release couldn't be found").unwrap(),
        vec![Node::Text("that release couldn't be found".to_string())]
    );
}

#[test]
fn simple_args() {
    assert_eq!(
        parse("Looking up barcode {position} of {total}").unwrap(),
        vec![
            Node::Text("Looking up barcode ".to_string()),
            Node::Arg("position".to_string()),
            Node::Text(" of ".to_string()),
            Node::Arg("total".to_string()),
        ]
    );
}

#[test]
fn plural_with_pound() {
    let nodes = parse("{count, plural, one {# pending delete} other {# pending deletes}}").unwrap();
    let Node::Plural { arg, cases } = &nodes[0] else {
        panic!("expected a plural node, got {nodes:?}");
    };
    assert_eq!(arg, "count");
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].selector, PluralSelector::Category(PluralCategory::One));
    assert_eq!(cases[0].message, vec![Node::Pound, Node::Text(" pending delete".to_string())]);
}

#[test]
fn plural_exact_and_category() {
    let nodes = parse("{n, plural, =0 {none} one {# thing} other {# things}}").unwrap();
    let Node::Plural { cases, .. } = &nodes[0] else {
        panic!("expected plural");
    };
    assert_eq!(cases[0].selector, PluralSelector::Exact(0));
    assert_eq!(cases.len(), 3);
}

#[test]
fn rejects_select() {
    let err = parse("{g, select, male {he} other {they}}").unwrap_err();
    assert!(err.contains("only `plural`"), "got: {err}");
}

#[test]
fn rejects_plural_without_other() {
    let err = parse("{n, plural, one {#}}").unwrap_err();
    assert!(err.contains("`other`"), "got: {err}");
}

#[test]
fn apostrophe_quotes_a_literal_brace() {
    assert_eq!(parse("a '{' b").unwrap(), vec![Node::Text("a { b".to_string())]);
}

#[test]
fn double_apostrophe_is_literal() {
    assert_eq!(parse("it''s here").unwrap(), vec![Node::Text("it's here".to_string())]);
}

#[test]
fn rejects_stray_close_brace() {
    assert!(parse("oops}").is_err());
}
