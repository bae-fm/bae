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

## Plan 3: Storage Trait + Implementation

**Goal**: Abstract storage behind a trait, implement the core structure.

- Define `ReleaseStorage` trait with `read_file`, `write_file`, `list_files`
- Create `ReleaseStorageImpl` that takes a `StorageProfile`
- Implement local raw case first (encrypted: false, chunked: false, location: Local)
- Refactor one consumer to use the trait

---

## Plan 4: Chunked/Encrypted Storage

**Goal**: Add chunking and encryption to the single storage implementation.

- Add chunking logic to `ReleaseStorageImpl` (when `chunked: true`)
- Add encryption logic to `ReleaseStorageImpl` (when `encrypted: true`)
- Add S3 backend to `ReleaseStorageImpl` (when `location: Cloud`)
- Migrate existing chunk code into this

---

## Plan 5: Import Pipeline Integration

**Goal**: Storage profile selection during import.

- Add storage profile picker to import workflow
- Write releases to selected storage configuration
- Default profile for quick imports

