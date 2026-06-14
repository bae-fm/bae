# App State

## Two kinds of data

**Core-owned** — core is the source of truth (database or runtime
state). The UI holds in-memory mirrors that the reducer keeps in sync
from core's typed event streams. Initial state may come from a
one-shot core query at startup or an on-demand load, but ongoing
updates flow only through events. Core-owned stores: `PlaybackStore`,
`ConfigStore`, `LibraryStore`.

**Session** — ephemeral to the UI session, alive only while the user
is in the app. Held in `ImportStore` and `UiStore`. Views are the
primary writers (direct for single-field, methods for compound). The
reducer calls UiStore methods for two narrow cross-cutting concerns
— global error display (`showError` / `clearError`) and post-removal
cleanup of view-domain selections (clearing `selectedReleaseIdByAlbum`
when its release is deleted) — and writes ImportStore's event-driven
fields directly (scan progress, identify state, file availability,
preview state). Views write ImportStore's user-set fields (mode,
selectedCoverUrl). The UI may call core via async request/response to
fetch derived facts (e.g. search results, prefetched release detail),
but core never pushes durable updates and doesn't own this data.

## Stores

| Store | Fields | Writer |
|---|---|---|
| `PlaybackStore` | `nowPlaying`, `volume`, `isMuted`, `repeatMode`, `queueItems` | reducer (core-owned) |
| `ConfigStore` | `config`, `syncStatus` | reducer (core-owned) |
| `LibraryStore` | `albumSummaries`, `releaseSummaries`, `releaseDetails` | reducer + paginated-list ingest + on-demand loaders |
| `ImportStore` | candidates (folder), `previewState` | reducer for event-driven fields (scan, identify, file availability, preview state); views for user-set fields (mode, selectedCoverUrl) |
| `UiStore` | navigation, overlays, search popover, `lastError` | views, plus reducer methods for global errors and post-removal cleanup |

Single-writer rule applies per **field**, not per store: reducer-written
fields and view-written fields coexist in `ImportStore` because each
field has exactly one writer.

Multi-field operations on `UiStore` and `ImportStore` go through methods
so they stay atomic and call sites describe intent. Single-field writes
go direct — the SwiftUI idiom for `@Observable` properties.

## Objects in the environment

**AppService** and **UiStore** are both placed in the environment via
`.environment(...)`.

**AppService** — root object. Holds bridge handle (`appHandle`), the
core-owned and import stores (`playbackStore`, `configStore`,
`libraryStore`, `importStore`), a reference to `uiStore` for
background-task error reporting (e.g. `storeRestoreCodeInKeychain`,
`filePath(for:)` surface failures via `uiStore.showError`), and action
methods. Not `@Observable` — views access stores through
`appService.<store>` and commands through `appService.appHandle`.

Stores own Combine subjects for ephemeral signals from their own
domain that don't fit `@Observable` — high-frequency streams (playback
position) or one-shot notifications (library shape
changed). Cross-cutting subjects with no single-store home live on
`AppService`.

## Reading from stores

Non-UI code (persistence, snapshots) must not read from the core-owned
stores. Query core through the bridge instead.

## Event flow

All core-owned state: service channel → UiEventBus → BridgeUiEvent
→ reducer → the appropriate store. The reducer is one function per
platform, switching on BridgeUiEvent. No per-invocation callbacks.
No mappings on the bus.

Each service has one broadcast channel for its events; the bus
subscribes once per service. Import service: `ImportEvent` (scan,
progress, identify, errors — all carry `candidate_key`, so the bus
routes events to candidates without any mapping state). Playback
service: `PlaybackProgress`.
Bridge dispatch tasks for per-operation channels (e.g. auto-identify)
emit to the bus the same way services do.

Core emits events only for data it owns and manages — its database,
its runtime state. Data core merely proxies (MusicBrainz lookups,
Discogs lookups, etc.) is owned by the UI session while the user has
it: the UI calls core via async request/response, the result lives in
`UiStore`, and core sends no updates because it has nothing to update.

High-frequency events (playback position, preview position):
reducer → Combine subjects (on the matching
store, or on `AppService` if cross-cutting); NSViewRepresentable
subscribers consume the subjects and update the AppKit view directly,
avoiding per-tick SwiftUI re-renders. Never as `@Observable` fields.

## No optimistic writes

The FFI round-trip (bridge call → core → event → reducer) is
sub-millisecond — no latency to optimize for. Stores never cross-write.
