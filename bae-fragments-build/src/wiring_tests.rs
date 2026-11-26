//! Each crate that holds the library's schema runs this build step, checks
//! against the schema it is built for, and hands its fragments on.

use super::{check_crate, read_file};
use std::path::{Path, PathBuf};

/// The crates whose build scripts run the step: bae-store, and every crate
/// bae-library's `schema.toml` lists.
fn stepped_crates(root: &Path) -> Vec<String> {
    let members = bae_schema_manifest::members(&|path| read_file(&root.join(path))).unwrap();
    std::iter::once("bae-store".to_owned()).chain(members.crates).collect()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn each_crate_that_holds_the_schema_runs_the_step_in_its_build_script() {
    let root = workspace_root();
    for name in stepped_crates(&root) {
        let build = read_file(&root.join(&name).join("build.rs")).unwrap();
        assert!(build.contains("bae_fragments_build::emit();"), "{name}: build.rs");
        let manifest = read_file(&root.join(&name).join("Cargo.toml")).unwrap();
        let links = format!("links = \"{}_sql\"", name.replace('-', "_"));
        assert!(manifest.contains(&links), "{name}: Cargo.toml lacks `{links}`");
        assert!(
            manifest.contains("bae-fragments-build = { workspace = true }"),
            "{name}: Cargo.toml lacks the build dependency"
        );
    }
}

#[test]
fn each_crate_that_holds_the_schema_passes_the_step_over_its_own_sources() {
    let root = workspace_root();
    for name in stepped_crates(&root) {
        let checked = check_crate(&name, &root.join(&name), &root, &[])
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            checked.schema_files.iter().any(|file| file.ends_with("schema.toml")),
            "{name}: the step read no schema"
        );
    }
}
