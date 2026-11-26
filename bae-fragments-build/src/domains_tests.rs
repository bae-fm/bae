use super::*;

const POLICY: &str = r#"
rules = ["database-boundary"]

[database]
capability = "SQLite"

[[database.domains]]
name = "records"
owner = "bae_records::Records"
homes = ["bae-records/src/records.rs", "bae-records/src/records/"]
tables = ["albums", "tracks"]

[[database.domains]]
name = "ratings"
owner = "bae_records::Records"
homes = ["bae-records/src/ratings.rs"]
tables = ["ratings"]
"#;

fn domains() -> Domains {
    Domains::from_policy(POLICY).unwrap()
}

fn foreign(table: &str, domain: &str) -> ForeignRead {
    ForeignRead { table: table.into(), domain: domain.into() }
}

#[test]
fn a_domains_homes_read_its_tables_and_the_tables_no_domain_lists() {
    for file in ["bae-records/src/records.rs", "bae-records/src/records/fragments.rs"] {
        let reads = domains().foreign_reads(
            file,
            ["albums", "tracks", "coven_files", "json_each"].map(String::from),
        );
        assert_eq!(reads, [], "{file}");
    }
}

#[test]
fn a_domain_does_not_read_another_domains_table_in_the_same_crate() {
    let reads = domains()
        .foreign_reads("bae-records/src/ratings.rs", ["ratings", "albums"].map(String::from));
    assert_eq!(reads, [foreign("albums", "records")]);
}

#[test]
fn a_file_in_no_domains_homes_reads_no_domain_table() {
    // A home that names a file holds no other file whose path it begins.
    for file in ["bae-records/src/views.rs", "bae-records/src/records.rs.orig"] {
        let reads = domains().foreign_reads(file, ["ratings"].map(String::from));
        assert_eq!(reads, [foreign("ratings", "ratings")], "{file}");
    }
}

#[test]
fn homes_of_two_domains_that_hold_one_file_are_refused() {
    for home in ["bae-records/src/records.rs", "bae-records/src/records/ratings.rs"] {
        let policy = POLICY.replace("bae-records/src/ratings.rs", home);
        let error = Domains::from_policy(&policy).unwrap_err();
        assert!(error.contains(home), "{error}");
    }
}

#[test]
fn a_table_in_two_domains_is_refused() {
    let policy = POLICY.replace("tables = [\"ratings\"]", "tables = [\"ratings\", \"albums\"]");
    let error = Domains::from_policy(&policy).unwrap_err();
    assert!(error.contains("albums"), "{error}");
}
