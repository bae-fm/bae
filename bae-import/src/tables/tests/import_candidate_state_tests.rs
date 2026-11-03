use super::*;
use crate::tables::import_state::tests::*;
use crate::tables::*;
use bae_util::content_type::ContentType;

use bae_catalogs::catalog::LookupCatalog;
use bae_db::test_fixtures::{empty_db, fixed_now};
use bae_db::watched_path::host_root;
use bae_db::{Database, DbArtist};
use bae_identify::TerminalVerdict;
use rusqlite::params;

#[path = "import_candidate_state_tests/catalog_release_retention.rs"]
mod catalog_release_retention;

include!("import_candidate_state_tests/folder_state.rs");
include!("import_candidate_state_tests/superseding.rs");
include!("import_candidate_state_tests/first_found.rs");
include!("import_candidate_state_tests/pane_rows.rs");
include!("import_candidate_state_tests/metadata_author.rs");
include!("import_candidate_state_tests/bindings.rs");
