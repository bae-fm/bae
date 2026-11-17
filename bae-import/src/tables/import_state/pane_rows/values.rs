//! A candidate's draft values besides its editable fields: the roles the
//! source credits on each track and on the release, and the works the tracks
//! perform with the works they nest among. They hang off the draft row, and
//! each track's off its track row; `tables::draft_rows` writes and reads
//! them. Their tables are `values.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("values.sql");

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
