use super::*;
use std::path::PathBuf;

fn declared(name: &str) -> Declared {
    Declared {
        name: Name::parse(name).unwrap(),
        file: PathBuf::from("Shots.kt"),
        needs_window: false,
    }
}

fn sentences(root: &Path, shots: &[Declared]) -> Vec<String> {
    problems(root, shots, &Record::default()).into_iter().map(|p| p.sentence).collect()
}

#[test]
fn preview_rows_are_read_as_state_ids() {
    let html = r#"W('Label', x), N('Nested, off', y), W('It\'s on', z), h('div')"#;
    assert_eq!(states(html), ["label", "nested-off", "it-s-on"]);
}

#[test]
fn a_component_shot_needs_a_row_of_that_name() {
    let root = bae_util::test_files::TestFolder::new();
    root.write(
        "design/system/components/MacRow/preview.html",
        "W('Label', a), W('Nested, off', b)",
    );
    root.write("design/system/components/AndroidCastRow/preview.html", "<p>no rows</p>");
    let shots = [
        declared("mac.component.row.label"),
        declared("mac.component.row.nested-off"),
        declared("mac.component.row.missing"),
        declared("android.component.cast-row.default"),
        declared("android.component.cast-row.looking"),
        declared("android.component.nothing.default"),
    ];
    let problems = sentences(root.path(), &shots);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].contains("mac.component.row.missing"));
    assert!(problems[0].contains("label, nested-off"));
    assert!(problems[1].contains("android.component.cast-row.looking"));
    assert!(problems[1].contains("default"));
    assert!(problems[2].contains("AndroidNothing/preview.html"));
}

#[test]
fn a_screen_shot_needs_its_board_file() {
    let root = bae_util::test_files::TestFolder::new();
    root.write("design/mocks/mac.welcome.libraries.dc.html", "<x-dc></x-dc>");
    let shots = [declared("mac.welcome.libraries"), declared("mac.welcome.create-name")];
    let problems = sentences(root.path(), &shots);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("design/mocks/mac.welcome.create-name.dc.html"));
}

#[test]
fn a_record_entry_needs_its_board_too() {
    let root = bae_util::test_files::TestFolder::new();
    root.write("design/mocks/mac.welcome.libraries.dc.html", "<x-dc></x-dc>");
    let mut record = Record::default();
    let entry = || crate::record::Entry {
        board: "b".to_string(),
        shot: "s".to_string(),
        findings: Vec::new(),
        warnings: Vec::new(),
    };
    record.set("mac.welcome.libraries", entry());
    record.set("mac.welcome.gone", entry());
    record.set("not a name", entry());
    let found = problems(root.path(), &[], &record);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|p| !p.declared));
    assert_eq!(found[1].name, "not a name");
    assert!(found[0].sentence.contains("design/shots.toml has an entry 'mac.welcome.gone'"));
    assert!(found[0].sentence.contains("mac.welcome.gone.dc.html is missing"));
}

#[test]
fn a_shared_component_is_the_board_of_each_platforms_shot() {
    let root = bae_util::test_files::TestFolder::new();
    root.write("design/system/components/Artwork/preview.html", "W('Default', a)");
    root.write("design/system/components/MacRow/preview.html", "W('Label', a)");
    root.write("design/system/components/Row/preview.html", "W('Label', a)");
    let component = |name: &str| match Name::parse(name).unwrap() {
        Name::Component(c) => board_component(root.path(), &c),
        Name::Screen(_) => unreachable!(),
    };
    assert_eq!(component("mac.component.artwork.default"), "Artwork");
    assert_eq!(component("mac.component.row.label"), "MacRow");
    assert_eq!(component("mac.component.nothing.default"), "MacNothing");
    let shots =
        [declared("mac.component.artwork.default"), declared("android.component.artwork.default")];
    assert!(sentences(root.path(), &shots).is_empty());
}
