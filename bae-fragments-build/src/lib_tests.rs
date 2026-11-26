use super::foreign_reads_in;
use crate::domains::TableOwners;

const POLICY: &str = r#"
[database]
capability = "SQLite"

[[database.domains]]
name = "records"
owner = "bae_records::Records"
homes = ["bae-records/src/records.rs"]
tables = ["albums"]

[[database.domains]]
name = "people"
owner = "bae_people::People"
homes = ["bae-people/src/people.rs"]
tables = ["ratings"]
"#;

#[test]
fn a_call_that_reads_another_crates_table_names_the_call_and_the_owner() {
    let owners = TableOwners::from_policy(POLICY).unwrap();
    let source = "sqlive_compose::fragment!(Rated, r#\"\n SELECT r.rating FROM ratings AS r JOIN albums AS a ON 1\n\"#);";
    let lines = foreign_reads_in("bae-people", &owners, "src/fragments.rs", source).unwrap();
    assert_eq!(
        lines,
        ["bae-people/src/fragments.rs:1: fragment! Rated reads albums, a table of records (bae-records); read it through a fragment of bae-records"]
    );
}

#[test]
fn a_call_that_reads_its_own_tables_and_spreads_another_crates_fragment_passes() {
    let owners = TableOwners::from_policy(POLICY).unwrap();
    let source = "sqlive_compose::read!(Page, r#\"SELECT r.rating FROM ratings AS r JOIN @bae_records::Album AS a ON 1\"#);";
    assert_eq!(
        foreign_reads_in("bae-people", &owners, "src/page.rs", source).unwrap(),
        Vec::<String>::new()
    );
}
