// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
const AA_SHARED: &str = "a98ba9aa-a32b-4716-842f-5505dee028f0"; // was "aa-shared"
const ALBUM_1: &str = "9644b84d-94b2-4b3b-863a-d6583931920c"; // was "9fd7bfa8-3c7c-4026-8559-da66af02f636"
const ALBUM_1999: &str = "88f57246-3e65-4eb9-8d36-ee8d40326cfc"; // was "album-1999"
const ALBUM_2001_LOWER: &str = "88183677-683b-485e-8224-f6a328c233c7"; // was "album-2001-lower"
const ALBUM_2001_UPPER: &str = "a663cff7-fad7-45b1-8469-5f77af82ddb8"; // was "album-2001-upper"
const ALBUM_A: &str = "a67c03ad-425f-45e9-8279-0144c852aaa5"; // was "album-a"
const ALBUM_ARTIST_1: &str = "288a78d6-b93d-4b4e-8452-fb678e33c2e8"; // was "album-artist-1"
const ALBUM_JUNCTION: &str = "7e6f42e7-8952-48e6-89bf-d1bcc611176d"; // was "album-junction"
const ALBUM_NEW: &str = "7d40ec33-80aa-4ab5-8010-78b55943ad81"; // was "album-new"
const ALBUM_NULL: &str = "c6648d5a-617e-4b69-87da-b7f1c4fb5e65"; // was "album-null"
const ALBUM_OLD: &str = "d80af162-0f69-4558-803e-742f4089d486"; // was "album-old"
const ALBUM_PERCENT: &str = "7e9948c4-f2d0-4a73-8e5c-a885eda086ff"; // was "album-percent"
const ALBUM_PRIOR: &str = "05d41bd9-ace6-4c2e-832e-0ef657f0caf3"; // was "album-prior"
const ALBUM_UNDERSCORE: &str = "2dd55a3f-3208-4faf-8737-453f474074cb"; // was "album-underscore"
const ARTIST_1: &str = "6c441836-aef7-4239-8a84-5336c4cce52c"; // was "artist-1"
const ARTIST_A: &str = "d7d8141f-54ff-467d-8b60-4f34a4d2e528"; // was "artist-a"
const ARTIST_ABSENT: &str = "78420eae-1cd1-4a36-87ae-2a5556aa52aa"; // was "artist-absent"
const ARTIST_ALBUM: &str = "85f70840-aba5-4eb9-8e1a-0d319e53b798"; // was "artist-album"
const ARTIST_B: &str = "38fc314c-c130-4120-8ca9-38b870ccef3a"; // was "artist-b"
const ARTIST_C: &str = "1b4bafc9-0ece-4538-833e-4ff52feb6ef0"; // was "artist-c"
const ARTIST_COMPOSER: &str = "5412b7ad-bdc1-4561-8985-b6d6ef8a2880"; // was "artist-composer"
const ARTIST_EXCLUSIVE: &str = "529eb0a5-b0bd-4e28-8c21-77fe62f8c77d"; // was "artist-exclusive"
const ARTIST_EXTRA: &str = "7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8"; // was "artist-extra"
const ARTIST_PRIMARY: &str = "7cdf9a34-0746-472b-8c68-0a669c11f2f1"; // was "artist-primary"
const ARTIST_SHARED: &str = "44d4b0bf-fd8a-4145-8deb-aa676bb4212a"; // was "artist-shared"
const ARTIST_SOLO: &str = "49549823-0e72-4747-891e-ee50e1611e3a"; // was "artist-solo"
const ARTIST_VARIOUS: &str = "f862abf2-3b15-4518-889b-1996d7100201"; // was "artist-various"
const ARTIST_WORK_ONLY: &str = "b96d8066-777d-408d-8ae4-ed58c767e40c"; // was "artist-work-only"
const BLOB_1: &str = "222d362a-5ce1-45ff-8a54-341cde525c2c"; // was "blob-1"
const BLOB_2: &str = "b1b46178-280d-48d4-86b3-62b31c040179"; // was "blob-2"
const COMPOSER_A: &str = "5dcc4999-03bd-42cc-8d14-8bf0a05effa3"; // was "composer-a"
const COMPOSER_B: &str = "2b748d47-e5b7-4c40-8716-1e608b9dfc3d"; // was "composer-b"
const COMPOSER_C: &str = "80cd3a5e-7fb7-4766-8ec3-d8e86575743b"; // was "composer-c"
const COMPOSER_SOLO: &str = "4d93d615-4549-45d9-81d9-644f079d59bf"; // was "composer-solo"
const ENTRY_A: &str = "e2ebff4e-4ed0-4a73-88ed-93453a79b463"; // was "entry-a"
const FILE_A: &str = "c9a20987-a1bf-4afe-890e-635c6cc13363"; // was "file-a"
const FILE_NEW: &str = "48804352-31c6-4a7c-8f44-9ac4cc62abdf"; // was "file-new"
const REL_1: &str = "cccb6034-5922-40d2-8d0b-d94619230882"; // was "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e"
const REL_A: &str = "25b35e24-5ff2-45e7-88ca-dc3e06995053"; // was "rel-a"
const REL_B: &str = "cee9702c-4399-45e8-894e-34aa16788938"; // was "rel-b"
const REL_NEW: &str = "f3078482-3f35-4019-8ade-a04971532682"; // was "rel-new"
const REL_OLD: &str = "3113dc59-d689-4c8c-86e9-4a3ae1565563"; // was "rel-old"
const REL_ONE: &str = "35fa3546-ff78-4214-857a-d323014e4e2c"; // was "rel-one"
const REL_TWO: &str = "6f389f38-00da-41c6-8dbf-365b1f7823fe"; // was "rel-two"
const RELEASE_1: &str = "c0218676-4c47-4eb7-8d65-57a8d328c3d1"; // was "release-1"
const RELEASE_A: &str = "0252dedb-ee39-4547-8803-438dbeb57a64"; // was "release-a"
const RELEASE_B: &str = "64e79a1f-404a-4c34-809a-a3cb44bf1942"; // was "release-b"
const RELEASE_LONELY: &str = "fcf4be32-159f-4790-87a1-697700a74462"; // was "release-lonely"
const RELEASE_OTHER: &str = "ce596bd7-be97-4416-8b6d-47f315bae466"; // was "release-other"
const RELEASE_PRIOR: &str = "878449fa-3b87-44f5-8e6b-5af3d41ea386"; // was "release-prior"
const RELEASE_ROLE_A: &str = "9b72bbbf-621e-41ca-8930-1623b643a20d"; // was "release-role-a"
const RELEASE_Z: &str = "8aa66d48-65a0-42e4-8c1d-e7481e8c1861"; // was "release-z"
const TRACK_A: &str = "0482872e-d4bf-4080-8426-441a0a3e71fc"; // was "track-a"
const TRACK_B: &str = "04676261-1659-47b1-879c-2947c52f4a8d"; // was "track-b"
const TRACK_LONELY: &str = "03c41035-ce18-4fa0-8e83-c446df26a551"; // was "track-lonely"
const TRACK_NEW: &str = "d28100a4-a355-47d3-8d5d-5a7b80bc66fd"; // was "track-new"
const TRACK_OTHER: &str = "69e67928-545a-4dcf-8ae7-ef7778331231"; // was "track-other"
const TRACK_PERCENT: &str = "4dc8cde9-15fb-470d-802c-b7e5f1ccc63d"; // was "track-percent"
const TRACK_PRIOR: &str = "6e9ff639-e1b3-48bf-84c5-1cc1794f3f70"; // was "track-prior"
const TRACK_ROLE_A: &str = "fa0c8483-f09a-4b69-8903-b1ebcdc31322"; // was "track-role-a"
const TRACK_UNDERSCORE: &str = "b2930937-dae6-4719-8150-aa61422eeeac"; // was "track-underscore"
const TRACK_WORK_A: &str = "d410a973-6a19-4ad3-87d8-b0c8c13d6015"; // was "track-work-a"
const WORK_A: &str = "432c8996-8af0-43dc-868a-822a256f65c4"; // was "work-a"
const WORK_ARTIST_A: &str = "ec41a8cd-a9a4-473e-8b70-d78168aefd8e"; // was "work-artist-a"
const WORK_CHILD_A: &str = "f63d8e66-6a81-4a67-8005-1fbe870f27eb"; // was "work-child-a"
const WORK_PARENT_A: &str = "6b05af7a-ee0c-4f12-8938-1d5536697271"; // was "work-parent-a"

#[cfg(test)]
mod queue_ordering_tests;

#[cfg(test)]
mod store_file_helpers;

#[cfg(test)]
mod in_clause_chunking_tests;

#[cfg(test)]
mod aggregate_ordering_tests;

#[cfg(test)]
mod connection_boundary_tests;

#[cfg(test)]
mod readable_cloud_path_tests;

#[cfg(test)]
mod row_mapper_error_tests;

#[cfg(test)]
mod composer_mode_tests;

#[cfg(test)]
mod artist_mode_tests;

#[cfg(test)]
mod playback_state_load_tests;

#[cfg(test)]
mod import_candidate_state_tests;

#[cfg(test)]
mod import_list_tests;

#[cfg(test)]
mod injected_ids_tests;

#[cfg(test)]
mod queue_cover_tests;

#[cfg(test)]
mod live_query_tests;
