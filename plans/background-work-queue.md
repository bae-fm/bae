# Background work queue

This is the ordered queue for the single background worker. The worker reuses
these worktrees across focused branches:

- bae: `/Users/dima/dev/bae-wt-error-surfacing`
- coven: `/Users/dima/dev/coven-worktrees/relay14`

Each concern is researched, planned, implemented, audited, fixed, and committed
with local verification on its own branch. The main checkout audits each commit,
fast-forwards `main`, and pushes it directly. Do not create pull requests or
additional worktrees.

## Active work

### Cloud attachment startup delay

Find and remove the delay between opening the local library and attaching cloud
sync for a returning user.

Confirmed measurements:

- A representative attachment took 4,704 ms.
- Opening the protocol root took 184 ms across two provider requests.
- Binding the provider took 146 ms across one provider request.
- Setting up owner membership took 4,372 ms across six provider requests.
- A separate launch from the main checkout reproduced about 4.9 seconds of
  attachment time.

The deeper owner-membership probes are building in the bae worker worktree. A
launch from the main checkout does not contain those probes. After the
instrumented build completes, launch only:

`/Users/dima/dev/bae-wt-error-surfacing/bae-macos/bae/.build/derivedData/Build/Products/Debug/bae.app`

Use the measurement to identify the confirmed cause. Remove all diagnostic
instrumentation and local dependency overrides before committing the product
fix.

## Ordered queue

### Folder group actions and hit area

- Remove the inline **Combine as One Release** action from folder headers.
- Keep the action in the folder header context menu when the group can be
  combined.
- Increase folder header vertical padding so the entire row has an appropriate
  pointer target.

### Import hierarchy indentation

- Indent only candidates that are actually members of a folder group.
- Align candidates outside that group with the outer hierarchy guide rather
  than the grouped-child guide.

### Candidate selection viewport stability

- Preserve the sidebar viewport during ordinary candidate selection.
- Permit scrolling only for explicit navigation commands that reveal a target.
- Find the source among selection handling, row identity, pagination updates,
  and row-height changes; fix it at the owning layer.

### Identity mode navigation

- Do not show Find or Change release controls while File Tags is active.
- Make the Lookup segment switch to the inline Lookup view without opening a
  modal.
- Open release search only from an explicit Find or Change action inside
  Lookup.
- Model lookup presentation and settled import identity at their owning layer
  instead of duplicating candidate state in the view.
- Preserve an existing picked release and the row-level pending state while
  changing modes.

### Storage Manager inspector

- Keep the transfer list compact.
- Show details for only the selected release in a side inspector.
- Provide a close action that hides and clears the inspector without changing
  transfer state.

### Release-choice confirmation

- Keep the release-choice sheet open after the user selects a pressing.
- Show the existing spinner in that pressing row while its release data loads.
- Commit the selection and close the sheet together only after the load
  succeeds.
- On failure, keep the sheet open and surface the error.

### File-backed evidence provenance

- Show a Barcode badge on the exact image from which the barcode was extracted.
- Show a Disc ID badge on the exact `.log` or `.cue` source used to compute it.
- Keep these badges independent of whether a pressing has been selected.
- Model extracted-signal provenance separately from evidence that supports the
  selected pressing.

### Files table columns

- Show Length only when at least one displayed file has a meaningful duration.
- Show Role only when at least one displayed file has a role the user can
  change.
- Remove fixed role chips such as Document and Other.
- Give hidden-column width back to Name.

### Tracks table source-first layout

- Order track columns as Source, track number, Title, Artist, and Length.
- Put the playable source filename at the far left as the primary origin field.
- Preserve playback behavior, alignment, truncation, accessibility, and
  responsive column widths.
- Increase the visible play/pause glyph and its pointer target while keeping
  every playback state vertically centered in a stable row height.
- Keep descriptor-file and audio-association controls in the Source side of the
  table instead of occupying mapped release fields.
- Constrain long source names and picker labels without changing grid width,
  and keep file sizes on one horizontal line.
- Verify awaiting-release, unassociated descriptor, associated descriptor, and
  long-filename rows at constrained and wide table widths.
- Give Tracks and Files their real per-section column models if they currently
  share a grid shape that prevents the correct order.

### Candidate placement breadcrumb

- Render the candidate header as a placement breadcrumb:
  `[Pending | Done | Skipped] > [folder icon] folder name [format]`.
- Source the placement from authoritative candidate state and update it when an
  import moves from Pending to Done.
- Make the placement root clickable. It switches the left sidebar to that list
  and reveals and selects the same candidate.
