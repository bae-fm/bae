//! Test stores on a schema, and the names coven gives a store's circles, for
//! the tests of this crate and every crate that builds on it.

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, people's and membership's.
pub const TEST_SCHEMA: bae_store::Schema =
    bae_store::Schema::new(&[bae_records::SCHEMA_PARTS, crate::SCHEMA_PARTS]);

// The name coven gives the circle `circle`.
coven::read!(
    writer CircleName(circle: coven::CircleId),
    "SELECT circle.name FROM coven_circles AS circle WHERE circle.circle = :circle"
);

/// The names coven gives the circle `circle` in `store`: one, or none when it
/// lists no such circle.
pub async fn circle_names(store: &bae_store::Store, circle: coven::CircleId) -> Vec<String> {
    let rows = store.read_rows(CircleNameParams { circle }).await.unwrap();
    rows.into_iter().map(|row| row.name).collect()
}
