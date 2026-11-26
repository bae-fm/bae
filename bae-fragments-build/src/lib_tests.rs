use super::foreign_reads;
use crate::domains::TableOwners;
use sqlive_compose::compiler::{CallKind, CompiledCall, Location};

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

fn call(kind: CallKind, name: &str, reads: &[&str]) -> CompiledCall {
    CompiledCall {
        kind,
        name: name.to_owned(),
        location: Location { file: "src/fragments.rs".to_owned(), line: 7, column: 1 },
        sql: String::new(),
        reads: reads.iter().map(|table| (*table).to_owned()).collect(),
    }
}

#[test]
fn a_call_that_reads_another_crates_table_names_the_call_and_the_owner() {
    let owners = TableOwners::from_policy(POLICY).unwrap();
    let calls = [call(CallKind::Fragment, "Rated", &["ratings", "albums"])];
    assert_eq!(
        foreign_reads("bae-people", &owners, &calls),
        ["bae-people/src/fragments.rs:7: fragment! Rated reads albums, a table of records (bae-records); read it through a fragment of bae-records"]
    );
}

#[test]
fn a_call_that_reads_only_its_own_tables_passes() {
    let owners = TableOwners::from_policy(POLICY).unwrap();
    let calls = [call(CallKind::Read, "Page", &["ratings"]), call(CallKind::Read, "Empty", &[])];
    assert_eq!(foreign_reads("bae-people", &owners, &calls), Vec::<String>::new());
}
