//! What a candidate's draft and a release editor's form share: the album a
//! draft files its release under, and its artist assignments as a
//! candidate's tables and a form's JSON both hold them. A candidate's draft
//! values are `import_state::draft_values`'; a release editor holds its form
//! itself, and its draft's reads bind the form's artists as JSON
//! ([`assignment_rows_sql`]).

mod album;
mod assignments;

pub(crate) use album::DraftAlbumRow;

pub(crate) use assignments::{
    assignment_kind_column, assignment_rows, assignment_rows_sql, assignments_json_sql,
    assignments_of, decoded, AssignmentRow, StoredAssignment,
};
