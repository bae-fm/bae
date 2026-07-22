# Owned queue rows

## Problem

The queue's "Playing From" lane is derived, not owned. Core keeps a recipe —
source + traversal (sequential, or a shuffle seed with an anchor) — and rebuilds
the lane from a fresh source fetch on every shuffle toggle and repeat wrap. User
edits (row removals, reorders) live in the entry vector, so a rebuild would
discard them; a patch set (`removed_track_ids`) re-subtracts removals after each
rebuild. Two authorities over one list:

- Unshuffle jumps to the playing track's album position, silently dropping
  every unplayed track that sits earlier in album order.
- Repeat wrap re-derives from the source, ignoring session edits.
- Reorders don't survive any rebuild at all — only removals got the patch set.
- "Clear" can't exist for the lane: clearing a derived list is incoherent, so
  the queue's Clear button only empties Up Next, and does nothing visible when
  Up Next is empty.

## Model

The queue is two owned lanes plus the current-track slot. Rows are the sole
authority over what plays; no operation rebuilds a lane from its source.

- **Up Next** (`manual`): unchanged. Explicit picks, drains first.
- **Playing From** (`context.entries` + cursor): filled once when a source
  starts playing. After the fill, every operation — remove, reorder, shuffle,
  unshuffle, repeat wrap, clear — is surgery on these rows.
- **Current** slot: unchanged; detached from both lanes.

`ContextSource` survives with exactly two consumers: the section label
("Playing From · Album Title") and the restart recipe. Nothing re-fetches it
during a session.

### Shuffle

`Traversal` is deleted. The context instead carries:

```rust
/// `Some` while the lane is shuffled: the full lane's entry ids in the order
/// they had when shuffle turned on. Shuffle-off rearranges the upcoming rows
/// into this relative order; ids since removed are simply absent. `None` =
/// sequential.
restore_order: Option<Vec<String>>,
```

- **Shuffle on**: record `restore_order` from the whole lane, then permute the
  *upcoming* rows in place. History and current don't move. No source fetch,
  no anchor — the playing track was never part of the permuted range.
- **Shuffle off**: rearrange upcoming rows to match their relative order in
  `restore_order`, drop it. History untouched. Removals stay gone; a reorder
  made before shuffling comes back exactly (the stamp is lane order, not album
  order). Everything unplayed still plays; nothing replays.

Recording the *whole* lane (not just upcoming) keeps unshuffle well-defined
after a repeat wrap moves previously-played rows back into upcoming.

Rows cannot join the context lane mid-session (external drops land in Up
Next), so every upcoming row is always present in `restore_order`.

### Repeat wrap (`RepeatMode::Context`)

Wrap loops the owned lane: cursor back to row zero. While shuffled, re-permute
the whole lane first (a fresh pass), keeping `restore_order` as-is. No source
fetch; removals and reorders are respected by construction.

### Side pause

The physical-side pause currently gates on `Traversal::Sequential`; the gate
becomes `restore_order.is_none()`. Everything else it reads is row/track
metadata (side letter, medium, release id) and is unaffected. Wrap improves:
looping an unshuffled sided release lands on side A's first row and prompts
"flip back" like a real record.

### Clear

- `clear()` (exists): empties Up Next. Unchanged.
- `clear_context()` (new): drops the whole context — entries, cursor, source,
  label. Current track keeps playing; when it ends, Up Next drains, then
  playback stops.

### Persistence

The recipe survives as the restart story (edits are session-scoped, as today):
`playback_state` stores source, current track id, shuffled flag, and the manual
lane's track ids. The shuffle seed and anchor columns are deleted — an owned
lane's exact order isn't reproducible from a seed once edited or wrapped, so
restart restores a pristine lane instead:

- Sequential: lane in source order, cursor at the current track's position
  (history = the album prefix, so Previous works).
- Shuffled: current track first, remainder freshly permuted behind it, cursor
  zero (shuffle history doesn't survive restart; Previous starts empty).

Pre-1.0: fold the column change into migration 001; `rm -rf ~/.bae`.

Persisting edited rows faithfully (storing the id list) stays possible later
without reshaping any of the above; it costs a rewrite of a potentially
library-sized blob per edit and is deliberately out of scope.

## Changes by layer

### bae-core

- `playback/context.rs`: delete `Traversal`, `shuffled_traversal`,
  `removed_track_ids`; add `restore_order`. `ContextStart::Shuffled` keeps its
  meaning (fill then permute the whole lane, cursor zero) without carrying a
  seed to persistence.
- `playback/queue.rs`:
  - `set_shuffle` becomes synchronous lane surgery (no track re-fetch — the
    service's shuffle path loses its source fetch entirely).
  - Repeat-wrap advance: cursor reset + optional re-permute, no re-fetch.
  - `next_sequential_context_track`: gate on `restore_order.is_none()`.
  - New `clear_context()`.
  - `remove()` context arm: drop the patch-set insert.
- `playback/service`: shuffle + wrap command paths lose their fetch plumbing;
  `clear_context` command added and echoed via `QueueUpdated` like every other
  queue mutation.
- `playback/persisted.rs`, `service/state.rs`, `db`: snapshot shape per
  Persistence above; migration 001 edited in place.

### bae-bridge

- `clear_context()` on the handle (additive; `BridgePlaybackContext` is
  unchanged — `shuffled` already crosses as a bool).

### UI

- BaeKit `Queue`: `clearContext` closure.
- macOS `QueueView`: header Clear removed; "Up Next" header gains Clear
  (section only renders with rows); "Playing From" header gains Clear beside
  the shuffle toggle. Preview fake (`PreviewQueueModel`) mirrors both.
- Android/iOS/Windows: no UI work in this change; the bridge addition is
  additive and `clearQueue` semantics are unchanged.

## Tests (core, failing-first where they pin bugs)

- Shuffle on permutes only upcoming; history, current, and Up Next unmoved.
- Unshuffle restores relative pre-shuffle order, including a user reorder made
  before shuffling; removals never resurrect (no patch set to get wrong).
- Unshuffle keeps every unplayed row (the six-vanished-tracks case from the
  recipe model is the regression test).
- Wrap loops the edited lane; shuffled wrap re-permutes and unshuffle
  afterward still restores pre-shuffle relative order.
- Side-pause gate: closed while shuffled, open after unshuffle, prompts on the
  wrap boundary of a sided release.
- `clear_context` leaves current playing, empties the projection, and stops
  advance after Up Next drains.
- Restore paths: sequential rebuilds prefix history; shuffled fronts current
  with fresh permutation.

## Delivery

Two PRs: core + bridge + persistence first (the model change), then the macOS
per-section Clear UI on top of it.
