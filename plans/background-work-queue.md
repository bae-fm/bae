# Background work queue

This is the ordered queue for the single background worker. The worker reuses
these worktrees across focused branches:

- bae: `/Users/dima/dev/bae-wt-error-surfacing`
- coven: `/Users/dima/dev/coven-worktrees/relay14`

Each concern is researched, planned, implemented, audited, fixed, and committed
with local verification on its own branch. The main checkout audits each commit,
fast-forwards `main`, and pushes it directly. Do not create pull requests or
additional worktrees.

## Completed work

### Cloud attachment startup delay

Completed in `4a1bbf129`. Returning-user attachment fell from 4.734 seconds to
0.863 seconds while retaining protocol and replay-image validation.

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

## Completed queue items

### Folder group actions and hit area

Completed in `091f0712f`.

- Remove the inline **Combine as One Release** action from folder headers.
- Keep the action in the folder header context menu when the group can be
  combined.
- Increase folder header vertical padding so the entire row has an appropriate
  pointer target.

### Import hierarchy indentation

Completed in `c9c83e167`.

- Indent only candidates that are actually members of a folder group.
- Align candidates outside that group with the outer hierarchy guide rather
  than the grouped-child guide.

### Candidate selection viewport stability

Completed in `4da9a75c5`.

- Preserve the sidebar viewport during ordinary candidate selection.
- Permit scrolling only for explicit navigation commands that reveal a target.
- Keep the revealed target as the retained anchor for later page deliveries.

### Import hierarchy checkbox hit area

Completed in `2040d107a`.

- Keep Pending's selection checkbox inside the row and its hit-testing area.
- Preserve the member and nonmember content alignment established by the
  hierarchy indentation change.
- Apply hierarchy alignment to row content without displacing List-owned
  selection controls.

### Identity mode navigation

Completed in `0005f913e`.

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

Completed in `b5db987e4`.

- Keep the transfer list compact.
- Show details for only the selected release in a side inspector.
- Provide a close action that hides and clears the inspector without changing
  transfer state.

### Release-choice confirmation

Completed in `8cd52f717`.

- Keep the release-choice sheet open after the user selects a pressing.
- Show the existing spinner in that pressing row while its release data loads.
- Commit the selection and close the sheet together only after the load
  succeeds.
- On failure, keep the sheet open and surface the error.
- Prevent a late completion from dismissing a newer modal.

### File-backed evidence provenance

Completed in `6985a5cd9`.

- Show a Barcode badge on the exact image from which the barcode was
  extracted.
- Show a Disc ID badge on the exact `.log` or `.cue` source used to compute it.
- Keep these badges independent of whether a pressing has been selected.
- Keep extracted-signal provenance separate from evidence that supports a
  search result.
- Preserve every extracted value when one file supplies multiple values of the
  same signal kind.

### Files table columns

Completed in `45dd0408d`.

- Remove the Length and Role columns from the Files table.
- Remove fixed role chips such as Document and Other.
- Keep file sizes and evidence badges inline with Name and give the
  removed-column width back to it.
- Preserve actionable role choices inline without reserving a column for
  fixed labels.
- Give Files and Tracks explicit per-section column models.

### Tracks table source-first layout

Completed in `6727834ff`.

- Order track columns as Source, track number, Title, Artist, and Length.
- Put the playable source filename at the far left as the primary origin field.
- Keep playback behavior, alignment, truncation, accessibility, and responsive
  widths.
- Use a 12-point play or stop glyph in a stable 24-point pointer target.
- Keep descriptor-file and audio-association controls inside Source.
- Constrain long source and picker labels without changing the resolved grid,
  and keep file sizes on one line.
- Verify awaiting-release, associated and unassociated descriptor, long-name,
  constrained-width, and wide-width rows.

### Candidate placement breadcrumb

Completed in `c825b094d`.

- Render the candidate header as a placement breadcrumb:
  `[Pending | Done | Skipped] > [folder icon] folder name [format]`.
- Source the placement from authoritative candidate state and update it when an
  import moves from Pending to Done.
- Make the placement root clickable. It switches the left sidebar to that list,
  clears a stale filter, opens the target's folder group, and reveals and
  selects the same candidate after the exact list revision arrives.

### Hosted candidate viewport regression

Completed in `3dfc25eae`.

- Preserve the hosted viewport regression that verifies live page deliveries
  keep the visible candidate anchored.
- Keep viewport bookkeeping in a stable non-observable reference so geometry
  preference delivery cannot invalidate SwiftUI rendering.

### Import release header preview environment

Completed in `e14568a3e`.

- Keep the release-header preview's full view chain loadable when its commit
  action reaches the candidate runtime reader.
- Compose the existing import and candidate-reader preview environments at the
  release-header preview root so its `Importer` and runtime publisher follow
  the same dependency path as other candidate previews.

## Active and ordered queue

No queued items.
