use super::*;

#[test]
fn each_credit_prints_its_join_and_the_last_prints_none() {
    assert_eq!(
        credit_line([("Artist A", Some(" feat. ")), ("Artist B", Some(" & "))]),
        "Artist A feat. Artist B"
    );
}

#[test]
fn a_credit_stating_no_join_is_followed_by_a_comma() {
    assert_eq!(
        credit_line([("Artist A", None), ("Artist B", None), ("Artist C", None)]),
        "Artist A, Artist B, Artist C"
    );
}

#[test]
fn no_credits_print_nothing() {
    assert_eq!(credit_line(std::iter::empty()), "");
}

#[test]
fn removing_the_last_credit_drops_its_neighbours_join() {
    let mut joins = vec![Some(" feat. ".to_string()), None];
    joins.pop();
    drop_last_join(&mut joins, |join| join);
    assert_eq!(joins, [None]);
    assert_eq!(
        credit_line([("Artist A", joins[0].as_deref()), ("Artist C", None)]),
        "Artist A, Artist C"
    );
}
