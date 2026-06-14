# Release Storage State Machine

How a release's bytes are stored, and how a release moves between storage
states. State is implicit in DB columns, not a separate enum column;
`ReleaseStorageState` (in `album_detail.rs`) is derived from them.

## Library states

```
Local   -- no cloud home; every release is Unmanaged
Synced  -- a cloud home is connected; releases can be Unmanaged or Managed
```

"Managed" means "uploaded to the cloud home." Management therefore only exists
with a cloud home: a Local library has no Managed releases and exposes no
storage transitions.

## Release states

```
Unmanaged  -- files at unmanaged_path, not in the cloud
Pinned     -- in the cloud AND a local copy in storage/ (locally playable)
CloudOnly  -- in the cloud, no local copy (not locally playable until pinned)
```

`Uploading` is not a fourth state — it's an overlay derived from the count of
pending `cloud_outbox` upload rows for the release, orthogonal to the columns.

### Derivation from DB columns

`unmanaged_path NOT NULL ⇒ pinned_locally = false` (DB CHECK constraint).

| State     | unmanaged_path | pinned_locally |
|-----------|----------------|----------------|
| Unmanaged | set            | false          |
| Pinned    | null           | true           |
| CloudOnly | null           | false          |

`storage_state(unmanaged_path, pinned_locally)` is the single derivation point;
`available_storage_actions(state, has_cloud_home, has_pending_uploads)` returns
the actions the UI may offer (none without a cloud home; none while uploading).

## Transitions

```
   Unmanaged ──Manage──▶ Pinned ◀──Pin──── CloudOnly
       ▲                  │  ▲                 │
       └────Unmanage──────┘  └──Unpin─────────-┘
       └────────────────Unmanage────────────────┘   (also from CloudOnly)
```

- **Import** seeds a release directly into Unmanaged, Pinned, or CloudOnly per
  the user's storage-mode choice (`StorageMode`), as below.
- **Manage** (Unmanaged → Managed; cloud required): upload the files to the
  cloud home via the cloud outbox, optionally keeping a local copy (`pin`) and
  optionally deleting the originals at `unmanaged_path`.
  - `pin=true` → stage a verified copy in `storage/`, enqueue uploads that read
    from there, clear `unmanaged_path` + set pinned, then (if requested) delete
    the originals — safe, since `storage/` already holds them.
  - `pin=false` → enqueue uploads that read directly from the originals, leave
    `unmanaged_path` set; the upload-completion observer clears it once the last
    file uploads (→ CloudOnly). If deletion of the originals was requested, the
    intent is persisted (`delete_unmanaged_source_on_upload`) and the observer
    deletes them only after the last upload lands — never before, because the
    originals are the upload source.
- **Unmanage** (Managed → Unmanaged): read every file (from `storage/` if
  Pinned, else download from the cloud), write each to a user-chosen
  `unmanaged_path`, verify it durably, then point the release at that path and
  drop the cloud + local managed copies. The whole copy-out is all-or-nothing:
  any per-file failure aborts before a single delete is queued.
- **Pin** (CloudOnly → Pinned): download each file from the cloud into
  `storage/`, set pinned.
- **Unpin** (Pinned → CloudOnly): drop the local copy. Rejected unless a cloud
  home exists and the release has no pending uploads — i.e. a durable cloud copy
  is confirmed, so the local copy is never the only copy.

### Safety invariant

Every transition verifies a durable, length-checked (`bytes.len() ==
file_size`) copy exists at the destination before queuing any delete (cloud
outbox or local manifest); source/cloud deletes run only inside the
copy/upload success path. The local-deletion 30s grace covers only `storage/`
files; cloud-outbox deletes fire on the next sync cycle, so nothing relies on
that window.

### Import paths

Storage mode is chosen at import confirmation (only with a cloud home):

- **Unmanaged**: index in place, set `unmanaged_path`, no cloud.
- **Managed (pinned)**: copy to `storage/`, `pinned_locally=true`, enqueue
  uploads reading from `storage/`. Playable immediately; stays Pinned.
- **Managed (unpinned)**: no local copy; enqueue uploads reading from the
  original `source_path`. Becomes CloudOnly once uploaded.

## Implementation pointers

- `StorageMode` (`import/types.rs`): `Unmanaged | Managed { pin }` — the
  import-time selector.
- `ReleaseStorageState` / `ReleaseStorageAction` + the two derivation functions
  (`album_detail.rs`): the runtime state and the actions the UI renders.
- `TransferService` (`storage/local/transfer.rs`): `pin` / `unpin` / `manage` /
  `unmanage`, each emitting `TransferProgress`; `read_release_file_bytes` is the
  shared local-or-cloud reader with the length check.
- `ReleaseUploadObserver` (`sync/blob_plan.rs`): on the last upload of a release
  that still has `unmanaged_path`, clears it (→ CloudOnly) and, if the
  delete-source intent is set, deletes the originals first.
- Cloud uploads/deletes flow through the `cloud_outbox` and the sync loop's
  `process_uploads` / `process_deletes`; release file bytes are never carried by
  the coven changeset (which carries only `library_images`).

## Detecting upload completion

After removing an outbox upload entry, JOIN through `release_files` to check if
any uploads remain for the same release; zero remaining ⇒ that release's upload
is complete (drives the observer above).
