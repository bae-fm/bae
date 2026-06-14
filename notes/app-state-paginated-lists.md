# Swift store architecture

The Swift client holds library data as **normalized slices** — one identity map per entity type — and expresses paginated UI as **generic `PaginatedList<Row>`** views that read across those slices. There are no denormalized projections cached in the store; any fat row the UI needs is composed at render time by joining slices.

This is the client-side counterpart to the bae-core LM-resolution pattern (`library-manager-resolution.md`). That work makes the library layer return pre-assembled resolved types. This work makes the consumer arrange those types into normalized slices and consume them through a generic list abstraction.

Plan history: `plans/reactive-library-store-v3.md`.

---

## Terminology

- **Slice** — a keyed identity map, `[EntityId: Entity]`. Unordered. Holds every instance of that entity type the client currently knows about. Populated by **interning**.
- **Intern** — upsert the canonical instance of an entity in its slice. Idempotent. Returns the canonical instance so multiple readers share one `@Observable` object.
- **Paginated list** — an ordered, sparse view over one or more slices. Knows a total count and a sparse array of slots. Each slot is an entity id or nil (not fetched yet).
- **Page source** — protocol that knows how to count and fetch pages of a given row shape from a given query. Typically thin wrapper over an `AppHandle` method.
- **Ingest** — per-list closure that splits each fetched row across the affected slices.
- **Invalidate** — the single mutation-visibility primitive on `PaginatedList`. Re-runs the query and atomically swaps in the new shape. Old `ids` and `totalCount` stay visible during the refetch; the swap happens in one main-actor hop when the reload completes.

---

## The three slices (`LibraryStore`)

```
albumSummaries    : [AlbumId:   AlbumSummary]        // grid row shape
releaseSummaries  : [ReleaseId: ReleaseSummary]      // storage row / picker shape
releaseDetails    : [ReleaseId: ReleaseDetail]       // tracks / files / gallery
```

- `AlbumSummary` is `@Observable`. Grid cards read its fields at the leaf; a title rename repaints just the affected cards. Album rows display artist names from the denormalized `artistNames: String` field carried on the summary.
- `ReleaseSummary` is `@Observable`. Storage rows read its fields; a pin toggle or size update repaints just that row.
- `ReleaseDetail` is a struct. Wraps the canonical `ReleaseSummary` from the `releaseSummaries` slice — interning a detail also interns its summary, so every consumer of the detail shares the same identity-stable summary instance. Fat fields (tracks, files, gallery) are replaced wholesale on update.

### Composition

```
ReleaseDetail
   ├── summary: ReleaseSummary  (identity-stable, lives in `releaseSummaries` slice)
   ├── displayName, year, label, catalogNumber, country
   ├── tracks, trackGroups, totalDurationLabel
   └── files, imageFiles, galleryItems
```

A consumer holding a `ReleaseDetail` reads `detail.summary.pinnedLocally` and `detail.tracks` off the same value; the summary pointer continues to resolve to the canonical instance even after the detail is replaced.

---

## Reducer mutators

`LibraryStore` exposes a `handle*` mutator per library event; the
reducer invokes the matching one and writes one or more slices and
nothing else.

**Rule**: mutators do not read from one slice to decide what to write to another. Events carry everything they need — `handleAlbumAdded(album: BridgeAlbumDetail)` contains the album and every release (with tracks/files) for that album. Slice writes are a pure function of the event payload.

**Rule**: mutators do not notify lists. Lists are views over a query at a point in time, not subscribers (see below). Content-only mutations (title rename, pin toggle) propagate to visible rows for free via `@Observable`. Shape-changing mutations broadcast on `libraryShapeSubject` (handled in the reducer right after the mutator call).

Mutators today:

- `handleAlbumAdded` / `handleAlbumUpdated` — write `albumSummaries`, `releaseSummaries`, `releaseDetails`. Unconditional upserts; no gate on prior state.
- `handleAlbumRemoved` — remove album from `albumSummaries`; remove all of its releases from `releaseSummaries` + `releaseDetails`
- `handleReleaseAdded` — write `releaseSummaries` + `releaseDetails` via `internReleaseDetail`, and upsert the parent album via `internAlbumSummary` so `releaseIds` reflects the authoritative DB-ordered list carried in the event
- `handleReleaseUpdated` — write `releaseSummaries` + `releaseDetails` via `internReleaseDetail`
- `handleReleaseRemoved` — remove release from `releaseSummaries` + `releaseDetails`

### Exceptions and why

`handleAlbumRemoved` filters `releaseSummaries.values` to find the releases that need to cascade. The wire event carries only the album id, and the slice is the source of truth for "which releases belong to this album." A future bae-core change — emitting child `ReleaseRemoved` events before `AlbumRemoved` — would let the reducer drop this read. Not worth pursuing until another reducer wants the same thing.

---

## Writer paths

Three paths write into slices:

1. **Event reducers** — above. The primary path for sync-driven and user-driven library changes.
2. **Paginated list ingest** — page fetches from `PaginatedList.loadRange` call the per-list ingest closure, which interns each row into the appropriate slices. The list's ingest defines which slices a given row shape contributes to.
3. **On-demand loaders** — `loadReleaseDetail(releaseId:)` / `reloadReleaseDetail(releaseId:)` fetch one release's fat data via `AppHandle.findReleaseDetail` and intern it.

All three paths funnel through the `intern*` methods, which keep identity stable and fields up to date.

---

## `PaginatedList<Row>`

```swift
@MainActor
@Observable
final class PaginatedList<Row: Identifiable & Sendable> {
    private(set) var totalCount: Int = 0
    private(set) var generation: Int = 0          // bumped by invalidate()
    var loadEpoch: LoadEpoch { ... }              // instance identity + generation

    func loadInitial() async              // fetch count; clear loaded segments
    func loadRange(offset, limit) async   // fetch a page, ingest rows, cache the segment
    func invalidate()                     // re-fetch count, bump generation
    func idAt(_ position: Int) -> Row.ID? // id at a loaded position, else nil
}
```

- **Generation-tagged segments** — loaded positions are stored as sorted, non-overlapping segments tagged with the generation they were fetched at; there's no sparse pre-allocation, and `idAt(i) == nil` means position `i` isn't loaded. Consuming views render a placeholder for nil positions and key a per-row `.task(id:)` on the list's `loadEpoch` so loads restart when the list is swapped for a fresh instance (a new sort/filter) or invalidated. Keying on the row position alone wouldn't change across a swap, leaving the new list's rows stuck on placeholders.
- **Ingest closure** — passed at construction. For a list of `BridgeAlbum` rows, ingest calls `store.internAlbumSummary(row)`. For a list of `BridgeStorageRow` (release + parent album in one payload), ingest calls both `store.internReleaseSummary(row.release)` and `store.internAlbumSummary(row.album)`.
- **`invalidate()`** is the single mutation-visibility primitive. When a user action could change the list's shape (import, delete, metadata update that could move a sort key), the action handler calls `list.invalidate()` at the mutation site. The previously-loaded segments and `totalCount` stay visible to the UI while a background task re-fetches the count and bumps the generation; the now-restarted row tasks call `loadRange()`, which replaces the stale segments as rows are revisited. There is no wipe-and-reload flicker.
- **Content-only mutations** (pin toggle, rename) need no list call. `@Observable` repaints the affected row because its body reads slice fields at the leaf.

### Lists are views, not subscribers

A `PaginatedList` observes nothing. It holds a snapshot of the query's shape at the moment of the last `loadInitial` / `invalidate`. Slice mutations don't auto-grow the list; the list's `ids` array doesn't change on events. Currently-visible rows re-render via `@Observable` when their slice fields change, but the list's ordering / membership stay frozen until the mutator calls `invalidate()`.

### `libraryShapeSubject` and typed change payloads

`AppService.libraryShapeSubject` is a Combine `PassthroughSubject<LibraryShapeChange, Never>`. The reducer sends a typed variant for every shape-altering library event:

```swift
enum LibraryShapeChange {
    case albumAdded(albumId: String)
    case albumRemoved(albumId: String)
    case releaseAdded(albumId: String, releaseId: String)
    case releaseRemoved(albumId: String, releaseId: String)
}
```

Subscribers filter on what matters to their list shape:

- Library grid rows are albums, so it only invalidates on `.albumAdded` / `.albumRemoved`. Release-level changes don't move rows.
- Storage grid rows are releases keyed by album, so it invalidates on every variant — album changes move groups of rows, release changes move individual rows.

---

## Composition diagram

```
┌─────────────────────────────┐
│  albumSummaries             │◀──── library grid (PaginatedList<BridgeAlbum>)
│  [AlbumId: AlbumSummary]    │◀──── storage list (joins on album.id)
└─────────────────────────────┘◀──── album detail view (header / labels)

┌────────────────────────────┐
│  releaseSummaries          │◀──── storage list (PaginatedList<BridgeStorageRow>)
│  [ReleaseId:               │◀──── album detail view (release picker labels)
│     ReleaseSummary]        │
└────────────────────────────┘
           ▲
           │ (detail wraps summary)
           │
┌────────────────────────────┐
│  releaseDetails            │◀──── album detail view (tracks, files, gallery)
│  [ReleaseId: ReleaseDetail]│
└────────────────────────────┘
```

Storage rows write to two slices (releaseSummaries + albumSummaries) from one bridge call. Library grid rows write to one slice (albumSummaries). Album-detail reducers write to all three slices from one event.

---

## When to read at the leaf

SwiftUI `@Observable` tracks which view accessed which property. Reading a slice field in a parent view widens the re-render scope. Pass the store down and read at the leaf:

```swift
// Good
AlbumRowView(album: album)
  // inside: Text(album.title)  ← tracked field read at leaf
```

```swift
// Bad
let title = album.title          // parent now re-renders on any title change
AlbumRowView(title: title)
```

The same rule applies to composing across slices:

```swift
// Good: row view reads both slices at the leaf
StorageRowContent(release: release, album: album)
```

```swift
// Bad: resolve in the parent, subscribe parent to both
let album = store.albumSummaries[release.albumId]
StorageRowContent(release: release, album: album)
```

In practice this is a small difference — the parent is probably inside a `LazyVStack` that only builds visible rows — but the rule is consistent: lowest viable scope.

---

## See also

- `plans/reactive-library-store-v3.md` — the plan that introduced this architecture
- `notes/library-manager-resolution.md` — the core-side corollary (LibraryManager returns pre-assembled types)
- `bae-macos/bae/bae/Services/PaginatedList.swift` — the generic list
- `bae-macos/bae/bae/Services/LibraryStore.swift` — the slices and reducers
