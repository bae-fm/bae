// Fixture row ids, UUIDs because coven refuses any other synced primary key.
const ALBUM_1: &str = "9644b84d-94b2-4b3b-863a-d6583931920c";
const ARTIST_1: &str = "6c441836-aef7-4239-8a84-5336c4cce52c";
const RELEASE_1: &str = "c0218676-4c47-4eb7-8d65-57a8d328c3d1";

use crate::test_fixtures::*;

#[cfg(test)]
mod release_storage_action_tests;

#[cfg(test)]
mod sync_start_failure_tests;

#[cfg(test)]
mod personal_order_tests;

#[cfg(test)]
mod in_clause_chunking_tests;

#[cfg(test)]
mod aggregate_ordering_tests;

#[cfg(test)]
mod connection_boundary_tests;

#[cfg(test)]
mod row_mapper_error_tests;

#[cfg(test)]
mod album_listing_tests;

#[cfg(test)]
mod file_row_tests;
mod queued_upload_reads_tests;
mod storage_listing_tests;

#[cfg(test)]
mod fact_ids_tests;

#[cfg(test)]
mod live_query_tests;

mod local_foreign_keys;

mod schema_tests;

#[cfg(test)]
mod release_medium_tests;

#[cfg(test)]
mod artist_identity_tests;

#[cfg(test)]
mod release_records_tests;

#[cfg(test)]
mod release_list_rows_tests;
