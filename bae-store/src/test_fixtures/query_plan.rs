//! SQLite's plan for a generated statement, for a test that checks how a read
//! or a write finds its rows: by an index, or by reading a whole table; and
//! the statistics a test plants for SQLite's planner to plan by.

use crate::WriterRead;

/// The detail of each step of SQLite's plan for `statement` on `store`, run
/// with the statement's parameters, in plan order.
pub async fn query_plan<Q>(store: &crate::Store, statement: Q) -> Vec<String>
where
    Q: coven::Read<Handle = sqlive_reads::Writer> + Send + 'static,
{
    store.read(move |sql| Ok(plan_of(&sql, &statement)?)).await.expect("SQLite plans the statement")
}

/// The steps of the plan SQLite reads `statement` by on `sql`, one line per
/// step, with the values `statement` binds.
pub fn plan_of<S: coven::Statement>(
    sql: &impl WriterRead,
    statement: &S,
) -> rusqlite::Result<Vec<String>> {
    sql.query(&format!("EXPLAIN QUERY PLAN {}", S::SQL), statement.params().as_slice(), |row| {
        row.get::<_, String>(3)
    })
}

/// One step of a plan: its id, the id of the step it runs under, and what
/// it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanStep {
    pub id: i64,
    pub parent: i64,
    pub detail: String,
}

/// The steps of the plan SQLite reads `statement` by on `sql`, with the tree
/// they make.
pub fn plan_steps_of<S: coven::Statement>(
    sql: &impl WriterRead,
    statement: &S,
) -> rusqlite::Result<Vec<PlanStep>> {
    sql.query(&format!("EXPLAIN QUERY PLAN {}", S::SQL), statement.params().as_slice(), |row| {
        Ok(PlanStep { id: row.get(0)?, parent: row.get(1)?, detail: row.get(3)? })
    })
}

/// [`plan_of`] on a bare library a test opened on `conn`
/// (`Schema::open_bare_library`).
pub fn plan_on<S: coven::Statement>(conn: &rusqlite::Connection, statement: &S) -> Vec<String> {
    let mut explain = conn.prepare(&format!("EXPLAIN QUERY PLAN {}", S::SQL)).unwrap();
    explain
        .query_map(statement.params().as_slice(), |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// Whether the program SQLite runs `statement` by on `conn`, a bare library a
/// test opened (`Schema::open_bare_library`), opens the index `index` to
/// read it. Explaining the statement runs none of it.
pub fn opens_index<S: coven::Statement>(
    conn: &rusqlite::Connection,
    statement: &S,
    index: &str,
) -> bool {
    let root: i64 = conn
        .query_row("SELECT rootpage FROM sqlite_schema WHERE name = ?", [index], |row| row.get(0))
        .unwrap();
    let mut explain = conn.prepare(&format!("EXPLAIN {}", S::SQL)).unwrap();
    let program: Vec<(String, i64)> = explain
        .query_map(statement.params().as_slice(), |row| Ok((row.get(1)?, row.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    program.iter().any(|(opcode, page)| opcode == "OpenRead" && *page == root)
}

/// Plant on `conn` the planner's statistics of `table` holding `rows` rows,
/// in place of any it holds: each of the table's indexes picks out, by its
/// columns up to each one, the fewest rows `per_value` gives any of those
/// columns. SQLite plans every statement prepared after this by them.
pub fn plant_table_statistics(
    conn: &rusqlite::Connection,
    table: &str,
    rows: u64,
    per_value: impl Fn(&str) -> u64,
) {
    conn.execute_batch("ANALYZE").unwrap();
    conn.execute("DELETE FROM sqlite_stat1 WHERE tbl = ?", [table]).unwrap();
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM pragma_index_list(?)")
        .unwrap()
        .query_map([table], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for index in indexes {
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
            .unwrap()
            .query_map([&index], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let mut stat = rows.to_string();
        let mut picked = rows;
        for column in &columns {
            picked = picked.min(per_value(column));
            stat.push_str(&format!(" {picked}"));
        }
        conn.execute(
            "INSERT INTO sqlite_stat1 (tbl, idx, stat) VALUES (?, ?, ?)",
            [table, &index, &stat],
        )
        .unwrap();
    }
    conn.execute_batch("ANALYZE sqlite_schema").unwrap();
}
