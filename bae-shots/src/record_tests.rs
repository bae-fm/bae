use super::*;

fn entry(board: &str, shot: &str) -> Entry {
    Entry { board: board.to_string(), shot: shot.to_string(), warnings: vec!["w".to_string()] }
}

#[test]
fn a_record_round_trips_through_its_text() {
    let mut record = Record::default();
    record.confirm("android.welcome.libraries", entry("b1", "s1"));
    record.confirm("mac.settings.appearance", entry("b2", "s2"));
    let text = record.to_text().unwrap();
    assert!(text.starts_with("# Written by `bae-shots run`"));
    assert!(text.contains("[\"android.welcome.libraries\"]"));
    assert_eq!(Record::parse(&text, "x").unwrap(), record);
}

#[test]
fn an_unknown_key_in_an_entry_is_refused() {
    let text = "[\"a.b.c\"]\nboard = \"x\"\nshot = \"y\"\nwarnings = []\nnote = \"z\"\n";
    assert!(Record::parse(text, "x").unwrap_err().to_string().contains("unknown field"));
}

#[test]
fn a_missing_file_is_an_empty_record() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Record::load(&dir.path().join("shots.toml")).unwrap(), Record::default());
}

#[test]
fn equal_hashes_are_confirmed() {
    let mut record = Record::default();
    record.confirm("a.b.c", entry("b", "s"));
    assert_eq!(record.standing("a.b.c", "b", "s"), Standing::Confirmed);
}

#[test]
fn either_hash_changed_is_changed() {
    let mut record = Record::default();
    record.confirm("a.b.c", entry("b", "s"));
    assert_eq!(record.standing("a.b.c", "b2", "s"), Standing::Changed);
    assert_eq!(record.standing("a.b.c", "b", "s2"), Standing::Changed);
}

#[test]
fn a_pair_with_no_entry_is_new() {
    assert_eq!(Record::default().standing("a.b.c", "b", "s"), Standing::New);
}

#[test]
fn forgetting_drops_the_entry() {
    let mut record = Record::default();
    record.confirm("a.b.c", entry("b", "s"));
    record.forget("a.b.c");
    assert!(record.get("a.b.c").is_none());
}

fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn the_base_branchs_record_is_read_from_git() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["commit", "-q", "--allow-empty", "-m", "empty"]);
    // A base with no record is an empty record.
    assert_eq!(Record::at(root, "main").unwrap(), Record::default());
    let mut record = Record::default();
    record.confirm("a.b.c", entry("b", "s"));
    std::fs::create_dir_all(root.join("design")).unwrap();
    record.save(&root.join(FILE)).unwrap();
    git(root, &["add", FILE]);
    git(root, &["commit", "-q", "-m", "record"]);
    assert_eq!(Record::at(root, "main").unwrap(), record);
    assert_eq!(Record::at(root, "main~1").unwrap(), Record::default());
    // A branch that is not there fails loudly.
    assert!(Record::at(root, "nowhere").unwrap_err().to_string().contains("no such commit"));
}
