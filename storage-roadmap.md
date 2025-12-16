# Storage Layer Roadmap

## Overview

Refactor storage to support flexible, composable options per release.

### Storage Profile Model

```rust
struct StorageProfile {
    name: String,
    location: StorageLocation,  // Local(path) or Cloud(bucket)
    encrypted: bool,
    chunked: bool,
}

enum StorageLocation {
    Local(PathBuf),
    Cloud(S3Bucket),
}
```

All 8 combinations of flags are valid. Some are silly (chunked but not encrypted locally) but the code doesn't care—it applies transforms in sequence.

---

## Plan 1: File-Chunk Mapping ✅

**Goal**: Explicit mapping from files to chunks, replacing fragile offset calculations.

- ✅ Add `DbFileChunk` model with `file_id`, `chunk_id`, `chunk_index`, `byte_offset`, `byte_length`
- ✅ Add `file_chunks` table and DB methods
- ✅ Populate during import pipeline persist stage
- ✅ Refactor `serve_image_from_chunks` to use the mapping

---

## Plan 2: Storage Profiles Schema ✅

**Goal**: Data model for reusable storage configurations.

- ✅ Add `DbStorageProfile` model with location, encrypted, chunked flags
- ✅ Add `StorageLocation` enum (Local/Cloud)
- ✅ Add `storage_profiles` table
- ✅ Add `DbReleaseStorage` linking releases to profiles
- ✅ Add `release_storage` table
- ✅ CRUD methods for profiles

---

## Plan 3: Storage Trait + Implementation ✅

**Goal**: Abstract storage behind a trait, implement the core structure.

- ✅ Define `ReleaseStorage` trait with `read_file`, `write_file`, `list_files`, `file_exists`, `delete_file`
- ✅ Create `ReleaseStorageImpl` that takes a `StorageProfile`
- ✅ Implement local raw case (encrypted: false, chunked: false, location: Local)
- ✅ Unit tests for local raw storage

---

## Plan 4: Encrypted/Cloud Storage ✅

**Goal**: Add encryption and cloud storage to the trait implementation.

- ✅ Add encryption logic (when `encrypted: true`) - encrypts/decrypts transparently
- ✅ Add S3 backend (when `location: Cloud`) - uploads/downloads whole files

## Plan 4b: Chunked Storage ✅

**Goal**: Add chunking support to the trait.

- ✅ Add database access to `ReleaseStorageImpl` for persisting chunk records
- ✅ Implement `write_chunked` (split into chunks, encrypt each, store, track with DbFileChunk)
- ✅ Implement `read_chunked` (fetch chunks via mapping, decrypt, concatenate)
- ✅ All 8 combinations now supported

---

## Plan 5: Import Pipeline Refactor ✅

**Goal**: Import uses storage trait instead of hardcoded chunk pipeline.

- ✅ Add database parameter to `ImportService`
- ✅ Add `create_storage(profile)` method to create storage from profile
- ✅ Add `run_storage_import()` method that uses storage trait
- ✅ Add `FileProgress` variant to `ImportProgress` for per-file progress
- ✅ Add `storage_profile_id` to `ImportRequest` and `ImportCommand`
- ✅ Wire `do_import()` to use storage path when profile ID specified
- ✅ Add `storage_profile_id` signal to `ImportContext`
- ✅ Import requests read profile from context
- ✅ Track metadata persistence in storage import path

**Status**: Folder imports with a storage profile use `run_storage_import()`. Torrent/CD imports still use legacy pipeline. CUE/FLAC handling deferred.

---

## Plan 6: Storage Profile UI (TODO)

**Goal**: Allow users to create/select storage profiles.

- Create default profile at app startup (encrypted + chunked + cloud)
- Add storage profile dropdown to import confirmation step
- Add settings page for managing storage profiles

