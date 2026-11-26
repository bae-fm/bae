use super::*;

const POLICY: &str = r#"
rules = ["database-boundary"]

[database]
capability = "SQLite"

[[database.domains]]
name = "records"
owner = "bae_records::Records"
homes = ["bae-records/src/records.rs", "bae-records/src/records.sql"]
tables = ["albums", "tracks"]

[[database.domains]]
name = "people"
owner = "bae_people::People"
homes = ["bae-people/src/people.rs"]
tables = ["ratings"]
"#;

fn owners() -> TableOwners {
    TableOwners::from_policy(POLICY).unwrap()
}

#[test]
fn a_crate_reads_its_own_tables_and_the_tables_no_domain_lists() {
    let reads = owners().foreign_reads(
        "bae-records",
        ["albums", "tracks", "coven_files", "json_each"].map(String::from),
    );
    assert_eq!(reads, []);
}

#[test]
fn a_crate_that_reads_another_crates_table_is_named_with_the_owner() {
    let reads = owners().foreign_reads("bae-people", ["ratings", "albums"].map(String::from));
    assert_eq!(
        reads,
        [ForeignRead {
            table: "albums".into(),
            owner: "bae-records".into(),
            domain: "records".into()
        }]
    );
}

#[test]
fn a_crate_without_a_domain_reads_no_domain_table() {
    let reads = owners().foreign_reads("bae-views", ["ratings"].map(String::from));
    assert_eq!(reads.len(), 1);
}

#[test]
fn a_domain_with_homes_in_two_crates_is_refused() {
    let policy = POLICY.replace(
        "homes = [\"bae-people/src/people.rs\"]",
        "homes = [\"bae-people/src/people.rs\", \"bae-records/src/x.rs\"]",
    );
    let error = TableOwners::from_policy(&policy).unwrap_err();
    assert!(error.contains("people"), "{error}");
}

#[test]
fn a_table_in_two_domains_is_refused() {
    let policy = POLICY.replace("tables = [\"ratings\"]", "tables = [\"ratings\", \"albums\"]");
    let error = TableOwners::from_policy(&policy).unwrap_err();
    assert!(error.contains("albums"), "{error}");
}
