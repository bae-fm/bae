use super::*;
use std::collections::BTreeMap;

fn files(entries: &[(&str, &str)]) -> BTreeMap<PathBuf, String> {
    entries.iter().map(|(path, text)| (PathBuf::from(path), (*text).to_owned())).collect()
}

fn read(files: &BTreeMap<PathBuf, String>, path: &Path) -> Result<String, ManifestError> {
    files.get(path).cloned().ok_or_else(|| ManifestError::unreadable(path, &"no such file"))
}

fn library() -> BTreeMap<PathBuf, String> {
    files(&[
        (
            "bae-library/schema.toml",
            "crates = [\"bae-a\", \"bae-b\"]\ndesktop_only = [\"bae-b\"]\n",
        ),
        (
            "bae-store/schema.toml",
            "[[function]]\nstatic = \"S\"\nname = \"bae_s\"\narguments = 1\nversion = 1\ncompute = \"crate::s\"\n\
             [[collation]]\nstatic = \"C\"\nname = \"bae_c\"\nversion = 1\ncompare = \"crate::c\"\n",
        ),
        (
            "bae-a/schema.toml",
            "[[function]]\nstatic = \"D\"\nname = \"bae_d\"\narguments = 2\nversion = 1\n\
             compute = \"crate::d\"\ndesktop = true\n[[part]]\nsql = [\"a.sql\", \"b.sql\"]\n",
        ),
        ("bae-a/a.sql", "CREATE TABLE a (id TEXT);"),
        ("bae-a/b.sql", "CREATE TABLE b (id TEXT);"),
        ("bae-b/schema.toml", "[[part]]\nsql = [\"c.sql\"]\n"),
        ("bae-b/c.sql", "CREATE TABLE c (id TEXT);"),
    ])
}

#[test]
fn a_desktop_migration_runs_every_crates_sql_in_crate_order() {
    let files = library();
    let migration = migration(&|path| read(&files, path), true).unwrap();
    assert_eq!(
        migration.sql,
        ["CREATE TABLE a (id TEXT);", "CREATE TABLE b (id TEXT);", "CREATE TABLE c (id TEXT);"]
    );
    let functions: Vec<_> = migration.functions.iter().map(|f| (&*f.name, f.arguments)).collect();
    assert_eq!(functions, [("bae_s", 1), ("bae_d", 2)]);
    assert_eq!(migration.collations, ["bae_c"]);
}

#[test]
fn a_phone_migration_leaves_out_the_desktop_only_crates_and_functions() {
    let files = library();
    let migration = migration(&|path| read(&files, path), false).unwrap();
    assert_eq!(migration.sql, ["CREATE TABLE a (id TEXT);", "CREATE TABLE b (id TEXT);"]);
    let names: Vec<_> = migration.functions.iter().map(|f| &*f.name).collect();
    assert_eq!(names, ["bae_s"]);
}

#[test]
fn a_file_the_migration_names_but_cannot_read_is_refused_by_path() {
    let mut files = library();
    files.remove(Path::new("bae-a/b.sql"));
    let error = migration(&|path| read(&files, path), true).unwrap_err().to_string();
    assert!(error.contains("bae-a/b.sql"), "{error}");
}

#[test]
fn a_crate_outside_the_library_list_is_built_for_every_device() {
    let files = library();
    let members = members(&|path| read(&files, path)).unwrap();
    assert!(members.on_phone("bae-store"));
    assert!(members.on_phone("bae-a"));
    assert!(!members.on_phone("bae-b"));
}
