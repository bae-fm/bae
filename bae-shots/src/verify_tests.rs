use super::*;

fn name(text: &str) -> Name {
    Name::parse(text).unwrap()
}

fn hashes(pairs: &[(&str, &str)]) -> BTreeMap<Name, String> {
    pairs.iter().map(|(n, h)| (name(n), h.to_string())).collect()
}

fn record_with(board: &str, board_hash: &str, shot_hash: &str) -> Record {
    let mut record = Record::default();
    record.confirm(
        board,
        Entry { board: board_hash.into(), shot: shot_hash.into(), warnings: Vec::new() },
    );
    record
}

#[test]
fn a_pair_whose_two_hashes_equal_its_entry_is_skipped() {
    let names = [name("mac.welcome.a")];
    let record = record_with("mac.welcome.a", "b", "s");
    let (skipped, pending) = partition(
        &names,
        &hashes(&[("mac.welcome.a", "b")]),
        &hashes(&[("mac.welcome.a", "s")]),
        &record,
    );
    assert_eq!((skipped.len(), pending.len()), (1, 0));
}

#[test]
fn a_pair_is_rechecked_when_either_hash_changed() {
    let names = [name("mac.welcome.a")];
    let record = record_with("mac.welcome.a", "b", "s");
    for (board, shot) in [("b2", "s"), ("b", "s2")] {
        let (skipped, pending) = partition(
            &names,
            &hashes(&[("mac.welcome.a", board)]),
            &hashes(&[("mac.welcome.a", shot)]),
            &record,
        );
        assert_eq!((skipped.len(), pending.len()), (0, 1), "{board} {shot}");
    }
}

#[test]
fn a_pair_with_no_entry_is_checked() {
    let names = [name("mac.welcome.a")];
    let (skipped, pending) = partition(
        &names,
        &hashes(&[("mac.welcome.a", "b")]),
        &hashes(&[("mac.welcome.a", "s")]),
        &Record::default(),
    );
    assert_eq!((skipped.len(), pending.len()), (0, 1));
}

#[test]
fn a_pair_is_new_against_a_base_that_lacks_it_and_changed_against_one_that_differs() {
    let base = record_with("mac.welcome.a", "b", "s");
    assert_eq!(base.standing("mac.welcome.a", "b", "s"), Standing::Confirmed);
    assert_eq!(base.standing("mac.welcome.a", "b2", "s"), Standing::Changed);
    assert_eq!(base.standing("mac.welcome.b", "b", "s"), Standing::New);
}

#[test]
fn a_run_passes_without_findings_or_problems() {
    let mut outcome = Outcome {
        runtime_published_at: String::new(),
        checked: Vec::new(),
        skipped: Vec::new(),
        problems: Vec::new(),
        lists: Lists::default(),
        not_taken: Vec::new(),
    };
    assert!(outcome.passed());
    outcome.problems.push(Problem { name: "x".into(), declared: false, sentence: "s".into() });
    assert!(!outcome.passed());
}

#[test]
fn a_pair_with_a_finding_fails_the_run() {
    let finding = Finding::Missing(crate::compare::normalize::Entry {
        index: 0,
        kind: crate::shots::Role::Text,
        words: "x".into(),
        value: String::new(),
        enabled: true,
        selected: false,
    });
    let list = crate::shots::ElementList::parse(crate::shots::tests::LIST).unwrap();
    let outcome = Outcome {
        runtime_published_at: String::new(),
        checked: vec![Checked {
            name: name("mac.welcome.a"),
            board: list.clone(),
            shot: list,
            pairs: Vec::new(),
            offset: (0.0, 0.0),
            board_png: PathBuf::new(),
            shot_png: PathBuf::new(),
            findings: vec![finding],
            warnings: Vec::new(),
            against_base: None,
        }],
        skipped: Vec::new(),
        problems: Vec::new(),
        lists: Lists::default(),
        not_taken: Vec::new(),
    };
    assert!(!outcome.passed());
}
