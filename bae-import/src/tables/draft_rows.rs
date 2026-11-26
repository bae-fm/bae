//! What the reads of a candidate's draft and of a release editor's form
//! share: the album a draft files its release under, and the statement that
//! reads artist assignments back with their picked artists as the library
//! holds them. A candidate's draft values, and the columns its tables store
//! an assignment in, are `import_state::draft_values`'; a release editor
//! holds its form itself, and binds its artists as JSON
//! (`release_edit::form_assignments`).

mod album;
mod assignments;

pub(crate) use album::DraftAlbumRow;

pub(crate) use assignments::{
    assignment_kind_column, assignments_json_sql, assignments_of, decoded, StoredAssignment,
};
