use super::*;
use crate::Schema;

#[test]
fn a_cost_line_carries_its_bar_for_the_suite_to_read() {
    assert_eq!(
        cost_line("album_page", Duration::from_micros(12_500), Some(50.0)),
        "COST metric=album_page value=12.5000 unit=ms bar=50"
    );
}

#[test]
fn a_cost_line_without_a_bar_has_none() {
    assert_eq!(
        cost_line("write/retitle", Duration::from_millis(3), None),
        "COST metric=write/retitle value=3.0000 unit=ms"
    );
}

#[test]
fn the_median_of_runs_is_the_middle_one() {
    let runs = [5, 1, 9, 3, 7].map(Duration::from_millis).to_vec();
    assert_eq!(median(runs), Duration::from_millis(5));
}

/// A read of two statements is planned statement by statement, in the order
/// it ran them, each with its values written in and its text on one line.
#[tokio::test]
async fn plans_each_statement_a_read_ran_in_order() {
    let (db, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let lines = plans(&db, "two_statements", |sql| {
        sql.query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", ["nothing"], |row| {
            row.get::<_, i64>(0)
        })?;
        sql.query(
            "SELECT name
               FROM sqlite_master
              WHERE type = ?1",
            ["table"],
            |_| Ok(()),
        )?;
        Ok(())
    })
    .await;
    let first = lines
        .iter()
        .position(|line| line.contains(" n=0 "))
        .expect("the first statement has plan steps");
    let second = lines
        .iter()
        .position(|line| line.contains(" n=1 "))
        .expect("the second statement has plan steps");
    assert!(first < second);
    assert!(lines[first].starts_with(
        "PLAN metric=two_statements n=0 sql=SELECT count(*) FROM sqlite_master WHERE name = 'nothing' step="
    ));
    assert!(lines[second].contains("sql=SELECT name FROM sqlite_master WHERE type = 'table' step="));
}

/// A statement longer than 200 characters is cut there.
#[tokio::test]
async fn cuts_a_long_statement_at_two_hundred_characters() {
    let (db, _tmp) = crate::test_fixtures::temp_db(Schema::new(&[])).await;
    let long = format!("SELECT {}1", "1 + ".repeat(100));
    let lines = plans(&db, "long", move |sql| {
        sql.query_row(&long, [], |row| row.get::<_, i64>(0))?;
        Ok(())
    })
    .await;
    let sql = lines[0].split(" sql=").nth(1).unwrap().split(" step=").next().unwrap();
    assert_eq!(sql.chars().count(), 200);
}
