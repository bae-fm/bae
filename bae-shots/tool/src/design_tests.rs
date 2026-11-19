use super::*;

/// A design tree with a board that mounts a part that mounts another part.
fn tree() -> bae_util::test_files::TestFolder {
    let root = bae_util::test_files::TestFolder::new();
    root.write(
        "design/mocks/mac.a.b.dc.html",
        r#"<x-dc><dc-import name="mac.parts.one" hint-size="1,2"></dc-import></x-dc>"#,
    );
    root.write(
        "design/mocks/mac.parts.one.dc.html",
        r#"<x-dc><dc-import hint="x" name="mac.parts.two"></dc-import></x-dc>"#,
    );
    root.write("design/mocks/mac.parts.two.dc.html", "<x-dc>two</x-dc>");
    root.write("design/mocks/mac.parts.other.dc.html", "<x-dc>other</x-dc>");
    root.write(TOKENS, "tokens");
    root.write(BUNDLE_CSS, "css");
    root.write(BUNDLE_JS, "js");
    root.write("design/mocks/canvas.json",
        r#"{"v":3,"boards":{"mac.a.b.dc.html":{"w":690,"h":600,"x":0,"y":0,"title":"t","page":"p"}}}"#,
    );
    root
}

fn hash(root: &Path) -> String {
    board_hash(root, "mac.a.b", "runtime", "chrome-1").unwrap()
}

#[test]
fn the_frame_comes_from_the_canvas() {
    let root = tree();
    assert_eq!(frame(root.path(), "mac.a.b").unwrap(), Frame { w: 690, h: 600 });
    assert!(matches!(frame(root.path(), "mac.nothing"), Err(DesignError::NoFrame(_))));
}

#[test]
fn parts_are_reached_through_parts() {
    let root = tree();
    let reached: Vec<String> = reached(root.path(), "mac.a.b").unwrap().into_keys().collect();
    assert_eq!(reached, ["mac.a.b", "mac.parts.one", "mac.parts.two"]);
}

#[test]
fn a_part_that_is_missing_is_named() {
    let root = tree();
    root.remove("design/mocks/mac.parts.two.dc.html");
    let error = board_hash(root.path(), "mac.a.b", "r", "c").unwrap_err().to_string();
    assert!(error.contains("mac.parts.one imports mac.parts.two"), "{error}");
}

#[test]
fn the_hash_changes_with_a_mounted_part_and_not_with_an_unmounted_one() {
    let root = tree();
    let before = hash(root.path());
    assert_eq!(before, hash(root.path()));
    root.write("design/mocks/mac.parts.other.dc.html", "<x-dc>changed</x-dc>");
    assert_eq!(before, hash(root.path()));
    root.write("design/mocks/mac.parts.two.dc.html", "<x-dc>changed</x-dc>");
    assert_ne!(before, hash(root.path()));
}

#[test]
fn the_hash_changes_with_each_built_file_the_runtime_and_the_browser() {
    let root = tree();
    let before = hash(root.path());
    for built in [TOKENS, BUNDLE_CSS, BUNDLE_JS] {
        root.write(built, "changed");
        assert_ne!(before, hash(root.path()), "{built}");
        root.write(
            built,
            if built == TOKENS {
                "tokens"
            } else if built == BUNDLE_CSS {
                "css"
            } else {
                "js"
            },
        );
    }
    assert_eq!(before, hash(root.path()));
    assert_ne!(before, board_hash(root.path(), "mac.a.b", "other", "chrome-1").unwrap());
    assert_ne!(before, board_hash(root.path(), "mac.a.b", "runtime", "chrome-2").unwrap());
}

#[test]
fn the_ready_boards_are_the_lines_of_shots_txt() {
    let root = tree();
    root.write("design/mocks/shots.txt", "# ready\nmac.a.b\n\n  mac.c.d \n");
    assert_eq!(ready_boards(root.path()).unwrap(), ["mac.a.b", "mac.c.d"]);
}
