# App State

## Two kinds of data

**Core-owned** — core is the source of truth (database or runtime
state). The UI holds in-memory mirrors that core's event flow keeps in
sync — some fields directly, from the shared event dispatcher's
`BridgeUiEvent` handling; others through a `Projection` that re-queries
after an `.invalidated` event names the domain that changed (see
`notes/app-state-paginated-lists.md`). Initial state may come from a
one-shot core query at startup or an on-demand load, but ongoing
updates flow only through events. Core-owned stores: `PlaybackStore`,
`ConfigStore`, `LibraryStore`.

**Session** — ephemeral to the UI session, alive only while the user
is in the app. Held in `ImportStore` and `UiStore`. Views are the
primary writers (direct for single-field, methods for compound). The
shared event dispatcher calls `UiStore.showError`/`clearError` for
global error display, and writes `ImportStore`'s preview-state field
directly; `ImportStore`'s scan/identify fields are written by the
import-candidate projections instead. Views write `ImportStore`'s
user-set fields (mode, selectedCoverUrl). The UI may call core via
async request/response to fetch derived facts (e.g. search results,
prefetched release detail), but core never pushes durable updates and
doesn't own this data.

## Stores

| Store | Fields | Writer |
|---|---|---|
| `PlaybackStore` | `nowPlaying`, `volume`, `isMuted`, `repeatMode`, `queueItems` | the event dispatcher (core-owned); the queue fields also take a lag-recovery projection |
| `ConfigStore` | `config`, `syncStatus` | the config and sync-status projections (core-owned) |
| `LibraryStore` | `albumSummaries`, `releaseSummaries`, `releaseDetails` | paginated-list ingest, the release-detail projection, and on-demand loaders |
| `ImportStore` | candidates (folder), `previewState` | the import-candidate projections for event-driven candidate fields (scan, identify, file availability); the event dispatcher for `previewState`; views for user-set fields (mode, selectedCoverUrl) |
| `UiStore` | navigation, overlays, search popover, `lastError` | views, plus the event dispatcher for global errors |

Single-writer rule applies per **field**, not per store: core-written
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

Core-owned state reaches the UI two ways: service channel → UiEventBus
→ BridgeUiEvent → the shared event dispatcher → the appropriate store
for fields the event payload carries directly (one exhaustive switch
shared by every Apple platform, no per-invocation callbacks, no
mappings on the bus); or service channel → UiEventBus → an
`.invalidated` event naming the changed domain → a `Projection`
re-query → the appropriate store, for fields whose current value has
to be fetched rather than read off the event.

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

High-frequency events (playback position via the event dispatcher,
preview position via the macOS-only import tail) publish to Combine
subjects (on the matching store, or on `AppService` if cross-cutting);
NSViewRepresentable subscribers consume the subjects and update the
AppKit view directly, avoiding per-tick SwiftUI re-renders. Never as
`@Observable` fields.

## No optimistic writes

The FFI round-trip (bridge call → core → event → the event dispatcher
or a projection refresh) is sub-millisecond — no latency to optimize
for. Stores never cross-write.
