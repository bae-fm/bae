# The queue

How playback's queue behaves. Companion to `14-playback-architecture.md`
(which covers the audio pipeline); this note covers what plays next and why.

## Shape

The queue is two owned lanes and a current-track slot.

- **Current**: the track playing right now. It belongs to neither lane; lane
  operations never touch it.
- **Up Next** (the manual lane): tracks the user explicitly queued. Drains
  first, in the order shown.
- **Playing From** (the context lane): the tracks of whatever source the user
  pressed play on — a release, several releases, or the whole library — with a
  cursor marking how far playback has gotten. Rows after the cursor are
  upcoming; rows before it are history.

The rows are the single authority over what plays. The source that filled the
context lane is remembered (`ContextSource`) for exactly two things: the
section label ("Playing From · Album Title") and the restart recipe. Nothing
consults the source again during a session — every operation below is surgery
on the rows themselves.

## Filling

Pressing play on a source replaces the context lane wholesale: its tracks
become rows, the chosen track becomes current, the cursor sits at its
position. Playing shuffled fills the lane and permutes it, cursor at the
front. Up Next is untouched by fills.

Rows enter the context lane only at fill time. External drops and "Play
Next" / "Add to Queue" land in Up Next — mid-session, the context lane can
shrink and rearrange but never grow.

## Advancing

When the current track ends (or Next is pressed):

1. Up Next's front row plays next, if any.
2. Otherwise the row after the context cursor plays and the cursor advances.
3. Otherwise playback stops — unless repeat says otherwise (below).

Previous walks back over context history. Skip-to jumps directly to any row:
a manual row drains the manual prefix before it; a context row moves the
cursor.

## Editing

Both lanes support row removal and drag reorder; removing or reordering never
interrupts the current track. A removed row is gone for the rest of the
session — no later operation resurrects it, because no later operation
consults anything but the rows.

Each lane has its own Clear, in its section header, present only while the
section has rows:

- **Clear Up Next**: empties the manual lane.
- **Clear Playing From**: drops the whole context — rows, history, cursor,
  label. The current track keeps playing; when it ends, Up Next drains, then
  playback stops.

## Shuffle

Shuffle is a property of the context lane; Up Next order is always exactly
what the user arranged. The lane carries `restore_order: Option<Vec<entry
id>>` — `None` when sequential, and, while shuffled, the whole lane's row
order from the moment shuffle turned on.

- **On**: record `restore_order`, permute the upcoming rows in place. Current
  and history don't move.
- **Off**: rearrange the upcoming rows into their relative `restore_order`
  positions, then drop it. Ids that were removed while shuffled are simply
  absent; every unplayed row keeps its place in line. A reorder made before
  shuffling round-trips exactly, because the stamp is lane order — not album
  order.

Recording the whole lane (not just upcoming) keeps unshuffle well-defined
after a repeat wrap moves played rows back into upcoming.

## Repeat

- **Off**: play through and stop.
- **Track**: pin the current track.
- **Context**: when the lane's last row finishes, loop the lane: cursor back
  to row zero. While shuffled, the whole lane is re-permuted first — a fresh
  pass over the same rows. Removals and reorders carry into every pass.

## Physical-side pause

With `pause_between_sides` enabled, playback holds for confirmation when one
physical side ends and the next begins: the next track comes from the context
lane, the lane is sequential (`restore_order` is `None`), Up Next is empty,
both tracks are from the same release, and their side letters differ. The
prompt names the medium (vinyl or cassette) and the side that ended.

Shuffling closes this gate; unshuffling reopens it. Looping a sided release
under context repeat prompts at the wrap — the last side ended, flip back to
side A.

## Persistence

Edits are session-scoped. The `playback_state` row stores a recipe — source,
current track id, shuffled flag, and the manual lane's track ids — not the
edited rows. Restart restores a pristine lane:

- Sequential: source order, cursor at the current track's position, so
  history is the album prefix and Previous works.
- Shuffled: current track first, the rest freshly permuted behind it —
  shuffle order and history don't survive restart.

Faithfully persisting edited rows (storing the id list) is deliberately not
done: it would rewrite a potentially library-sized blob on every edit. The
model leaves it possible.

## Projection

Every mutation bumps a revision and pushes a `QueueUpdated` snapshot over the
bridge: the manual rows, the context's label and shuffled flag, a windowed
slice of upcoming context rows plus the total, and transport flags
(`hasNext`/`hasPrevious`). UIs render the snapshot and send commands back;
they hold no queue state of their own. Optimistic UI edits reconcile against
the next snapshot's revision.
