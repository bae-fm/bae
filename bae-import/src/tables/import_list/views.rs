//! The lists a surface has open: each list's view, the groups it folds and
//! the rows it selects, and the views its reads take its rows from.

pub(super) const SQL: &str = include_str!("views.sql");
