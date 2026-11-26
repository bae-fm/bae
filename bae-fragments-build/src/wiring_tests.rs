//! Each crate that holds a fragment or a read runs the build step, with the
//! `links` key that hands its fragments on, and no other crate runs it.

use super::read_file;
use std::path::{Path, PathBuf};

/// The crates of the library's schema: bae-store, and every crate
/// bae-library's `schema.toml` lists.
fn schema_crates(root: &Path) -> Vec<String> {
    let members = bae_schema_manifest::members(&|path| read_file(&root.join(path))).unwrap();
    std::iter::once("bae-store".to_owned()).chain(members.crates).collect()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[expect(
    clippy::disallowed_methods,
    reason = "file system: a test lists the folders of a crate's sources"
)]
fn rust_files(folder: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Whether the crate `name` writes a `fragment!` or `read!` call.
fn holds_a_call(root: &Path, name: &str) -> bool {
    let mut files = Vec::new();
    rust_files(&root.join(name).join("src"), &mut files);
    files.iter().any(|file| {
        let text = read_file(file).unwrap();
        text.contains("sqlive_compose::fragment!(") || text.contains("sqlive_compose::read!(")
    })
}

#[test]
fn each_crate_that_holds_a_call_runs_the_step_and_no_other_crate_does() {
    let root = workspace_root();
    let mut stepped = 0;
    for name in schema_crates(&root) {
        let build = read_file(&root.join(&name).join("build.rs")).unwrap();
        let manifest = read_file(&root.join(&name).join("Cargo.toml")).unwrap();
        let links = format!("links = \"{}_sql\"", name.replace('-', "_"));
        let wiring = [
            build.contains("bae_fragments_build::emit();"),
            manifest.contains(&links),
            manifest.contains("bae-fragments-build = { workspace = true }"),
        ];
        let holds = holds_a_call(&root, &name);
        stepped += usize::from(holds);
        assert_eq!(
            wiring, [holds; 3],
            "{name}: its build script, `links` key and build dependency go with a call"
        );
    }
    assert!(stepped > 0, "no crate holds a fragment or a read");
}
