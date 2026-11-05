//! What a schema's tests check of it whatever parts it is made of: each check
//! names what breaks the rule, which the test asserts is nothing.

use crate::Schema;

/// The tables `schema` creates that aren't `STRICT`, which store a value of
/// the wrong type rather than refuse it.
pub fn loose_tables(schema: Schema) -> Vec<String> {
    let conn = crate::schema::bare_library_connection(schema);
    let loose = conn
        .prepare(
            "SELECT name FROM pragma_table_list
             WHERE schema = 'main' AND type = 'table' AND NOT strict
               AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    loose
}

/// Each column an `UPDATE OF` trigger of `schema` names that its table
/// doesn't have, as `trigger: table.column`. SQLite accepts a name no column
/// has, and the trigger then never fires on it.
pub fn absent_update_of_columns(schema: Schema) -> Vec<String> {
    let conn = crate::schema::bare_library_connection(schema);
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
    missing
}
