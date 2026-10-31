const SCHEMA: &str = include_str!("../../../../migrations/001_initial.sql");

/// Every table the schema creates is `STRICT`, so a value of the wrong type
/// is refused rather than stored.
#[test]
fn every_table_is_strict() {
    let conn = crate::library::bare_library_connection();
    let loose: Vec<String> = regex::Regex::new(r"(?m)^CREATE TABLE IF NOT EXISTS (\w+)")
        .unwrap()
        .captures_iter(SCHEMA)
        .map(|captures| captures[1].to_string())
        .filter(|table| {
            let strict: i64 = conn
                .query_row(
                    "SELECT strict FROM pragma_table_list WHERE name = ?",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            strict != 1
        })
        .collect();
    assert!(loose.is_empty(), "tables not STRICT: {loose:?}");
}

/// Every column an `UPDATE OF` trigger names is a column of its table.
/// SQLite accepts a name no column has, and the trigger then never fires on
/// it.
#[test]
fn every_update_of_trigger_names_columns_its_table_has() {
    let conn = crate::library::bare_library_connection();
    let update_of = regex::Regex::new(r"(?is)UPDATE OF\s+(.+?)\s+ON\s+(\w+)").unwrap();
    let triggers: Vec<(String, String)> = conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE type = 'trigger'")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut checked = 0;
    let mut missing = Vec::new();
    for (trigger, sql) in &triggers {
        let Some(captures) = update_of.captures(sql) else {
            continue;
        };
        checked += 1;
        let table = &captures[2];
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_xinfo(?)")
            .unwrap()
            .query_map([table], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for named in captures[1].split(',').map(str::trim) {
            if !columns.iter().any(|column| column == named) {
                missing.push(format!("{trigger}: {table}.{named}"));
            }
        }
    }
    assert!(checked > 0, "no UPDATE OF trigger found");
    assert!(
        missing.is_empty(),
        "triggers name absent columns: {missing:?}"
    );
}
