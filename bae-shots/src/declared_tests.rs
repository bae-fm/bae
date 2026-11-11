use super::*;

#[test]
fn kotlin_reads_the_first_argument_of_shot_and_component() {
    let source = r#"
        @Test
        fun androidWelcomeLibraries() = shot("android.welcome.libraries") { Welcome() }

        @Test
        fun androidComponentRowTitle() =
            component(
                "android.component.row.title",
                wide = true,
            ) { Row() }

        protected fun shot(board: String, content: @Composable () -> Unit) {}
        // "android.welcome.not-a-call"
    "#;
    assert_eq!(kotlin(source), ["android.welcome.libraries", "android.component.row.title"]);
}

#[test]
fn swift_reads_string_literals_of_board_form() {
    let source = r#"
        welcomeWindow("mac.welcome.libraries.none", libraries: []),
        ShotScene(id: "mac.welcome.create-name", size: WelcomeWindow.size) {
        ShotScene(id: "appearance", size: size)
        let key = "macOS.version"
    "#;
    assert_eq!(swift(source), ["mac.welcome.libraries.none", "mac.welcome.create-name"]);
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn read_joins_both_harnesses_and_leaves_the_tests_out() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        &format!("{KOTLIN_DIRECTORY}/WelcomeShots.kt"),
        r#"fun a() = shot("android.welcome.libraries") {}"#,
    );
    write(
        root.path(),
        &format!("{KOTLIN_DIRECTORY}/ElementListTest.kt"),
        r#"fun a() = shot("android.test.known") {}"#,
    );
    write(
        root.path(),
        &format!("{SWIFT_DIRECTORY}/ShotScene+Welcome.swift"),
        r#"ShotScene(id: "mac.welcome.create-name")
           ShotScene(id: "mac.welcome.create-name")"#,
    );
    write(
        root.path(),
        &format!("{SWIFT_DIRECTORY}/ShotElementListTests.swift"),
        r#"list("mac.welcome.add-code.entering")"#,
    );
    let names: Vec<String> =
        read(root.path()).unwrap().iter().map(|d| d.name.to_string()).collect();
    assert_eq!(names, ["mac.welcome.create-name", "android.welcome.libraries"]);
}

#[test]
fn a_literal_that_is_not_a_board_name_fails_the_read() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        &format!("{KOTLIN_DIRECTORY}/WelcomeShots.kt"),
        r#"fun a() = shot("android.welcome") {}"#,
    );
    write(root.path(), &format!("{SWIFT_DIRECTORY}/ShotScene.swift"), "");
    assert!(matches!(read(root.path()), Err(DeclaredError::Name(..))));
}
