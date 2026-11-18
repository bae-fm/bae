//! The statements that write and read the values a draft holds beside its
//! own row: its label and barcode rows, its album and track artist
//! assignments, its mediums, and the roles and works of its release and its
//! tracks. Two owners hold these values, each in tables of its own with the
//! same columns: a candidate's draft (`import_state::pane_rows`) and a
//! release editor's session (`release_edit_session`). Each statement takes
//! the [`DraftTables`] it writes or reads.

mod album;
mod form_rows;
mod values;

pub(crate) use album::DraftAlbumRow;

pub(crate) use form_rows::{
    assignment_kind_column, assignments_json_sql, assignments_of, barcodes_json_sql, decoded,
    form_row_values, insert_album_artist_assignments, insert_barcode_rows, insert_label_rows,
    insert_medium_rows, insert_track_artist_assignments, labels_json_sql, mediums_json_sql,
    mediums_of, read_label_rows, store_album_artist_assignments, StoredAssignment, FORM_COLUMNS,
};
pub(crate) use values::{
    insert_draft_values, insert_track_values, release_roles_json_sql, roles_of,
    track_roles_json_sql, track_works_json_sql, work_parts_json_sql, works_json_sql, works_of,
    StoredRole,
};

/// The tables that hold one owner's draft values, and the column that keys
/// them: a candidate's draft by its content hash, or a release editor's
/// session by its id. Each table has the columns of the candidate's table of
/// the same values.
pub(crate) struct DraftTables {
    /// The column every table keys its owner by.
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) barcode: &'static str,
    pub(crate) album_artist: &'static str,
    pub(crate) track_artist: &'static str,
    pub(crate) medium: &'static str,
    pub(crate) track_role: &'static str,
    pub(crate) release_role: &'static str,
    pub(crate) work: &'static str,
    pub(crate) work_credit: &'static str,
    pub(crate) work_part: &'static str,
    pub(crate) track_work: &'static str,
}
