# Cloud upload user journey

## Status

### Implemented and verified

- Both import and Storage Manager hand off to the same coven outbox. The
  initiating surface remains in an in-flight state until the retained outbox
  revision contains the release, so it does not briefly return to Local.
- Coven's public outbox live query is the durable source for queued, prepared,
  retry-wait, uploaded, cancelling, and publishing work. bae reacts to both that
  query and its batch display-data query; missing release or filename context
  fails the projection.
- Source preparation and provider upload publish retained in-memory byte
  progress rather than writing progress ticks to SQLite. Coven samples buffer
  progress every 300 ms and sends the exact terminal count.
- Import rows, release storage state, Storage Manager rows, and queue sections
  consume the same bridge snapshot. Foreground storage actions remain visible
  until the retained queue takes ownership.
- The user-visible retry state is named Retrying, and its explanatory text says
  the work remains pending and will retry. Pause, retry, and cancellation
  controls dispatch absolute commands to coven.
- Durable enrollment and library installation are separate from this journey;
  their coven and bae changes have been committed, pushed, and exercised on
  Android and macOS.
- The public core and bridge upload-progress count is named `retrying` instead
  of `failed`, and the macOS, Android-fixture, and Avalonia consumers read that
  field. Core, bridge, Avalonia model and view, Android unit-test compile, and
  macOS baeTests gates pass against the combined checkout.

### Remaining evidence and work

- Run one provider-backed release through the Storage Manager entrypoint. The
  import entrypoint ran live against S3 (GCS): Nebraska, 9 files, 224.2 MB,
  Queued → Preparing → Uploading → Uploaded → Publishing → Cloud, exact
  221.2 MB provider denominator on the FLAC, publication landed on the next
  sync cycle (the ~30 s idle cadence bounds its latency).
- Findings from the live import run, each needing a fix at its owning layer:
  - The queue header and per-file bars are the two-stage work bar (completed
    preparation fills half), but their labels count provider bytes only
    ("3 MB of 224.2 MB" under a half-full bar). Bar and label must speak the
    same units.
  - The large FLAC sat at "Zero kB of 221.2 MB" while smaller files uploaded;
    provider byte samples appeared only after the first chunk acked.
  - Import's "Measuring loudness" step does not advance the side panel's
    progress.
- Exercise provider failure, retry, pause, resume, cancellation, and app restart
  against durable queue rows. Fix any phase, action, or progress disagreement
  with a failing test at its owning layer.
- Verify aggregate throughput and estimated completion start only when every
  provider denominator is known, reset after the active batch ends, and never
  reuse preparation bytes as provider bytes.
- Verify terminal publication removes the outbox release without a Local-state
  flash on the import row, release row, album details, Storage Manager, or queue
  panel.

### Environment facts learned while landing

- Android host-JVM unit tests that boot `BaeApp` (Robolectric) cannot run on
  the macOS dev host: the JNA AAR carries no host `libjnidispatch`, and there
  is no host build of `libbae_bridge` for the unit-test classpath. CI's ubuntu
  runner is where that suite actually runs. Making the suite host-runnable
  needs a host-feature bridge dylib plus desktop-JNA test wiring — its own
  concern.
- `xcodebuild test` fails with a LaunchServices `childPID > 0` assertion when
  run from a Background launchd session (tmux); the test host launches
  suspended and is never resumed. Running the same command from the `gui/501`
  domain (a one-shot LaunchAgent, or a shell in the Aqua session) works.

## Entry from Import

The user chooses Cloud, optionally Pinned, and confirms an import. The import
commit creates the release and every blob-bearing row before asking coven to
make the release remote. A successful result carries the outbox publication
revision that first contains the durable upload intent.

Until the local `OutboxStore` reaches that revision, the import row says it is
waiting for the cloud queue. At that revision the row renders the release's
real outbox phase and progress. If an entire upload completes between retained
watch deliveries, a newer revision with no matching release proves completion;
the row moves to Done instead of remaining queued.

## Entry from Storage Manager

The user chooses Move to Cloud on a Local release. The foreground action covers
precondition checks and the atomic coven enqueue. The durable outbox projection
must be published before the foreground action disappears, so the storage row,
album details, and queue panel never fall back to a resting Local state between
the two sources.

Both entrypoints then render the same `OutboxSnapshot`; neither owns another
upload state machine.

## Durable queue arrival

Coven atomically records the make-Remote intent and one upload row for every
blob. Its public cloud-outbox live query reports the complete current queue.
bae reconfigures a second live query over the exact release/file IDs in that
queue; that query batch-loads album titles and filenames and reacts when either
changes. Every durable or display update rebuilds from both current values, and
missing display context fails the projection. No partial or default-labelled
snapshot is published.

The UI displays Queued with exact source-byte totals. Restarting the app reads
the same durable rows, so queued, prepared, retrying, uploaded, cancelling, and
publishing work remains visible without waiting for a new callback.

## Source preparation

Coven reads the source, verifies it, and writes the durable upload spool as a
stream. Its observer samples plaintext bytes consumed every 300 ms and emits an
exact final count. bae stores those counts only in memory and publishes them
through a retained `watch` value stream; it does not write progress ticks to
SQLite or send them through the general UI event bus.

The queue header, release row, file row, import row, storage row, and album
details display Preparing. Their numerator is plaintext bytes consumed and
their denominator is the source row's exact plaintext size. Regressing counts,
changed denominators, or counts beyond the denominator fail loudly.

## Prepared provider object

When preparation commits, coven records Prepared and the exact provider-object
size in the durable queue. The UI displays Prepared even after restart.

Progress bars are phase-scoped: a bar always shows the current phase's bytes
against that phase's denominator, and its label counts the same units —
Preparing fills 0→100% in source bytes, Uploading fills 0→100% in provider
bytes, with the phase named beside the bar. No bar mixes phases into one
combined fill; a bar and its label never disagree about what is being counted.
(The earlier two-stage combined work bar rendered "3 MB of 224.2 MB" under a
half-full bar — a bar whose fill and label speak different units is the defect
this rule exists to prevent.)

## Provider upload

Coven streams the prepared object to the provider and samples encrypted bytes
accepted every 300 ms, followed by an exact final count. bae publishes these
transient counts through the same retained value stream used for preparation.
The provider numerator and durable provider denominator must agree; source size
is never substituted. Aggregate provider totals stay hidden until every queued
file has reached a phase whose exact stored size is known.

The UI displays Uploading, exact provider bytes, aggregate throughput over the
active rolling window, and an ETA only when every queued upload has an exact
provider denominator. Finishing or failing the last active transfer resets the
throughput lifecycle so a later batch does not inherit stale samples.

## Pause and resume

Pause is an absolute target command. Coven checks it before admitting each
upload entry; entries already in progress are allowed to finish. The snapshot
therefore has three effective states: Running, Pausing while that write remains
active, and Paused once no provider write is active. Throughput and ETA are
hidden after pause is requested, while the progress bar remains active during
Pausing and is dimmed only once fully Paused. Resume wakes the sync loop
immediately.

## Failure and retry

Coven durably records the failed attempt and error on the same upload row. The
live query moves every surface to Retrying and keeps the row present across
restart. Retry now clears the retry wait and wakes the loop; a failure to issue
that command is surfaced to the initiating UI. Progress for a new attempt
starts from zero with a new throughput measurement.

## Uploaded and publication

The final provider callback must include exact completed provider progress.
Coven then records Uploaded/Created durably. The UI keeps that file completed
inside the release group while the make-Remote gate is still Local.

After every blob lands, coven publishes the release transition atomically. The
queue displays Publishing even though there are no more byte counters, and all
storage actions stay suppressed. Once publication flips the release to Remote
and consumes the intent, the outbox live query removes the release. Release
subscriptions then render Cloud or Pinned from the authoritative storage state.

## Cancellation

Cancelling records and drains the unwind operation. Every surface displays
Cancelling without byte progress and suppresses other storage actions. The
transition is not reported complete until the make-Remote intent is absent; a
failed unwind is returned to the initiating UI rather than being left for a
later repair pass.

## Evidence required

- Core tests cover every phase, exact denominators, cadence ordering, pause
  transitions, retry/restart state, publication, cancellation, and terminal
  removal.
- Bridge tests prove every phase and counter crosses unchanged.
- Store and macOS tests prove retained revisions, fast completion, state labels,
  action suppression, and Import/Storage handoff.
- Localization generation validates every locale after user-visible state text
  changes.
- Targeted core, bridge, and macOS builds and tests pass against the combined
  checkout before the concern is committed.
