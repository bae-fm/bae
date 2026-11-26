use bae_store::test_fixtures::schema_checks::{
    absent_update_of_columns, keyless_tables, loose_tables,
};

/// Every table the schema creates is `STRICT`, so a value of the wrong type
/// is refused rather than stored.
#[test]
fn every_table_is_strict() {
    let loose = loose_tables(crate::SCHEMA);
    assert!(loose.is_empty(), "tables not STRICT: {loose:?}");
}

/// Every column an `UPDATE OF` trigger names is a column of its table.
/// SQLite accepts a name no column has, and the trigger then never fires on
/// it.
#[test]
fn every_update_of_trigger_names_columns_its_table_has() {
    let missing = absent_update_of_columns(crate::SCHEMA);
    assert!(missing.is_empty(), "triggers name absent columns: {missing:?}");
}

/// Every table has a primary key, so coven reruns a read only for a write to
/// a row it reads (spec 3.2): a table with none reaches every read of it for
/// every write.
#[test]
fn every_table_has_a_primary_key() {
    let keyless = keyless_tables(crate::SCHEMA);
    assert!(keyless.is_empty(), "tables with no primary key: {keyless:?}");
}
