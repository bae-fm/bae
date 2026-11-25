use super::*;

#[test]
fn an_unknown_key_is_refused_by_name() {
    let error =
        crate_schema("bae-records", "[[part]]\nsql = [\"a.sql\"]\nfunctons = [\"crate::A\"]\n")
            .unwrap_err()
            .to_string();
    assert!(error.contains("bae-records/schema.toml"), "{error}");
    assert!(error.contains("functons"), "{error}");
}

#[test]
fn synced_without_namespaces_is_refused_by_name() {
    let error = crate_schema(
        "bae-people",
        "[[part]]\nsql = [\"a.sql\"]\nsynced = { tables = \"crate::synced::synced_tables\" }\n",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("bae-people/schema.toml"), "{error}");
    assert!(error.contains("namespaces"), "{error}");
}

#[test]
fn a_desktop_function_is_gated_where_a_part_lists_it() {
    let rust = crate_schema(
        "bae-records",
        r#"
[[function]]
static = "LENGTHS_DISAGREE"
name = "bae_lengths_disagree"
arguments = 2
version = 1
compute = "crate::lengths_disagree"
desktop = true

[[part]]
sql = ["a.sql"]
functions = ["crate::LENGTHS_DISAGREE", "bae_store::NAME_KEY"]
"#,
    )
    .unwrap();
    assert!(rust.contains("#[cfg(desktop)]\npub(crate) static LENGTHS_DISAGREE"), "{rust}");
    assert!(
        rust.contains("PART_1_FUNCTIONS: &[&bae_store::SchemaFunction] = &[\n#[cfg(desktop)]\n&crate::LENGTHS_DISAGREE,\n&bae_store::NAME_KEY,"),
        "{rust}"
    );
}

#[test]
fn the_library_gates_its_desktop_only_crates() {
    let rust = library_schema(
        "crates = [\"bae-records\", \"bae-import\"]\ndesktop_only = [\"bae-import\"]\n",
    )
    .unwrap();
    assert!(rust.contains("bae_records::SCHEMA_PARTS,\n#[cfg(desktop)]\nbae_import::SCHEMA_PARTS,"));
}
