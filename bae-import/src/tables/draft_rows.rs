//! The statements that read the values a draft holds beside its own row:
//! its label and barcode rows, its album and track artist assignments, its
//! mediums, and the roles and works of its release and its tracks. A
//! candidate's draft (`import_state::pane_rows`) holds them in its tables,
//! which `import_state::draft_writes` writes; a release editor holds its form
//! itself, and its draft's reads bind the form's artists as JSON
//! ([`assignment_rows_sql`]).

mod album;
mod form_rows;
mod values;

pub(crate) use album::DraftAlbumRow;

pub(crate) use form_rows::{
    assignment_columns, assignment_json_rows, assignment_kind_column, assignment_rows_sql,
    assignments_json_sql, assignments_of, barcodes_json_sql, decoded, form_row_values,
    labels_json_sql, mediums_json_sql, mediums_of, read_label_rows, StoredAssignment, FORM_COLUMNS,
};
pub(crate) use values::{
    release_roles_json_sql, roles_of, track_roles_json_sql, track_works_json_sql,
    work_parts_json_sql, works_json_sql, works_of, StoredRole,
};
