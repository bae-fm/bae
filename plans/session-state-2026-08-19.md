# Session state — cloud upload journey + sync performance (2026-08-19, ~01:00)

Working doc for resuming after context compaction. Two repos: bae
(/Users/dima/dev/bae, main checkout stays on main) and coven
(/Users/dima/dev/coven). User email dminkovsky@gmail.com; phone = Pixel over
adb (5C241JEA320842).

## Mission

plans/cloud-upload-user-journey.md is the driving plan. Tonight's session:
landed the retrying rename, ran the live provider-backed import journey,
fixed everything it exposed. Remaining journey items:

- Storage Manager entrypoint (import as Local → Move to Cloud) — not yet run.
- Provider failure / retry / pause / resume / cancel / restart battery.
- Throughput + ETA gating checks.
- Terminal publication no-Local-flash check.

## Landed tonight — bae main (all pushed; main = b7bfa050 + maybe newer)

- 651697e5 rename upload progress failed→retrying (all gates green)
- 92b19b62 Reconnect fix (PR #359, merged): reconnect_sync core→bridge→BaeKit
- bdba2311 cargo-deny allow bae-fm/rustls-platform-verifier fork (Linux CI fix)
- f0f50228 dotnet format whitespace (Windows CI fix)
- c931e97e device-pairing approval failure logging
- 25cb9eaa pin coven 4e31c274 (Android flock store lock — fixed phone's
  "try_lock() not supported"; std's try_lock is a stub on Android)
- 253a5b20, e68ef8fa plan doc updates (live-run notes; phase-scoped bar design)
- 6bcfc10e outbox snapshot: tolerate feed skew between durable rows and
  transient callbacks (fixed live panic crash-loop at outbox_snapshot.rs:827)
- 67dce618 pin coven 879e1f38 (stage timings + streamed GCS upload progress)
- 106641a6 phase-scoped progress bars (bar+label same units, work_* deleted,
  BridgeUploadBar; all platforms + 30 locales)
- b7bfa050 loudness scan advances the import sidebar percent

## Landed tonight — coven main (= 3cde3ae4, pushed)

- 4e31c274 Android store lock via rustix flock
- 7163ffb2 per-stage sync timing logs (StageTimings; Stopwatch in foundation clock.rs)
- 879e1f38 GCS upload streams body in 256KiB chunks with live byte reports
  (GCS S3 endpoint = single PUT, never multipart — was one report at end)
- 3cde3ae4 verification-artifact read boundary gate:
  --verification-artifact-boundary in owner-construction-check (new
  method_patterns field + ExprMethodCall visitor), gates read_protocol_object
  to reviewed homes, wired into scripts/check-owner-boundaries.sh, failure
  message names plans/verified-reuse-consolidation.md. Committed with
  CARGO_TARGET_DIR=$HOME/.cargo-target/coven (see pitfalls).

## Measured performance findings (live GCS store)

- Publish latency per release: drain→publish 20–75s. Breakdown of a 105s
  cycle: drain uploads 37.2s (real bytes), pull 38.6s of which
  **prepare retained history 38.3s** (re-verifies whole retained merge
  history from cloud EVERY cycle), **publish pending writes 25.9s** (one
  serial GCS PUT per prepared object, ~40 objects/release), acks 2s.
- Survey (agent, complete, in coven plans/verified-reuse-consolidation.md):
  20 reuse mechanisms across ~10 artifact kinds; all seven "Reuse verified X"
  commits are per-cycle only (tests all say "_within_a_cycle"); durable
  checkpoint path exists and outbound uses it with zero cloud reads; pull
  ignores it. Six duplicate-mechanism pairs recorded in that plan's backlog.

## Agents in flight (check ListAgents / task notifications)

1. **coven agent** (worktree /Users/dima/dev/coven-wt-timings): working the
   retained-history fix — pull must adopt durable retained checkpoints
   (RetainedReplayCache + VerifiedMergeMembershipPrefix::from_retained, like
   authorize_retained_outbound), stop cloud verify_refs over retained refs;
   add "second cycle does no cloud reads" test class; add stage timings
   inside the publish path (the 26s); shrink the new gate's allowlist if
   homes go dead; rebase onto 3cde3ae4. Told: boundary gate already built by
   me — do NOT create one. Merge protocol: it pushes branches, I ff-merge
   (no PRs), then bump bae's coven pin + rebuild apps + remeasure publish.
2. **iOS reconnect agent** (worktree /Users/dima/dev/bae-wt-sync-reconnect,
   branch fix-ios-sync-reconnect): wiring reconnectSync into iOS
   SettingsView/LibraryView + config gate. Was mid iphonesimulator build;
   got its build dirs swept once. Status unknown — poke it, then ff-merge.
3. Bars agent: DONE, both branches merged (106641a6, b7bfa050).

After coven agent's current task: hand it the consolidation backlog from
coven plans/verified-reuse-consolidation.md as serial single-concern
branches (duplicate ack caches, founder×3, registrations×4, baseline cache
bypass at materialization_io.rs:447/:640, owner-anchor memo), ending with
shrinking the gate allowlist toward one owner module. User approved this
direction.

## Live test environment

- macOS app: Debug build from main checkout, RUNS VIA LaunchAgent
  gui/501/fm.bae.logged (wrapper
  /private/tmp/claude-501/.../scratchpad/run-bae-logged.sh, log
  scratchpad/bae-app.log with RUST_LOG=info,coven=debug,bae_core=debug).
  Relaunch: `launchctl kickstart -k gui/501/fm.bae.logged`. Currently
  running the phase-bars build. A monitor watches the log for ERROR/panic.
- Library: fresh (~/.bae wiped tonight), S3-compatible = GCS bucket
  bae-import-dima, store prefix lwerkjvwe/. Mac = Owner 86523287, phone =
  Member f2c5cf96 (pairing works end-to-end). Albums synced to phone,
  cloud streaming playback works.
- Phone build: current main + coven 879e1f38, includes an UNCOMMITTED
  diagnostic log in bae-android AlbumDetailScreen.kt (the only dirty file in
  the bae checkout — "detail state:" logcat line). OPEN BUG: first tap into
  an album shows header-only screen; back+reenter loads. Repro pending —
  read logcat for "detail state" when user reproduces. Static analysis so
  far: delivered detail with no matching selectedReleaseId, or first
  delivery empty; suspect stale initialReleaseId or first-emission shape.
- Reinstall app on phone: bae-bridge/build-android.sh --abi arm64-v8a, then
  cd bae-android && JAVA_HOME=$(brew --prefix openjdk@21)/libexec/openjdk.jdk/Contents/Home
  ANDROID_HOME=$HOME/Library/Android/sdk ./gradlew installFullDebug
  --no-daemon -Pbae.abi=arm64-v8a.

## Environment pitfalls (hard-won tonight)

- `xcodebuild test` crashes (childPID>0 assert) from tmux/Background
  session; run via one-shot LaunchAgent in gui/501. Unsigned builds
  (CODE_SIGNING_ALLOWED=NO) can't read the keychain → keyring/S3-cred
  failures; user-facing app builds must be signed (omit the CODE_SIGNING
  flags; team BTNNN8533W).
- CARGO_TARGET_DIR: every harness shell inherits ~/.cargo-target/bae (from
  bae cwd at shell init). For ANY coven build/commit, set
  CARGO_TARGET_DIR=$HOME/.cargo-target/coven or checkouts thrash each
  other's binaries (this bit the boundary-gate commit: hook kept running a
  stale owner-construction-check built by the agent's worktree).
- Disk: reclaim-disk launchd sweeper is DISABLED (user wants manual
  management; it comes back on next login). When low: delete
  ~/.cargo-target/*/debug/incremental first (tens of GB, regenerable), then
  /private/tmp non-claude dirs, then idle target dirs (target-android is
  disposable when nothing android is building; keep target-macos and the
  app's derivedData while testing). NEVER delete derivedData of the running
  app or dirs agents are building in. Check df before big builds.
- Coven pre-commit reformats then asks to re-commit — just run git commit
  again (files stay staged). Both repos: plans/ needs `git add -f`.
- Android host JNA: Robolectric tests can't run on macOS (no host
  jnidispatch/bridge dylib) — CI-only; fixing that is a recorded separate
  concern (bae plan doc, Environment facts).
- bae CI: was all-red pre-existing; deny + whitespace fixed tonight. iOS
  periphery dead-code failure unexamined. Android CI = the ubuntu JNA
  failure class, may persist — check latest run on next push.

## Immediate next steps on resume

1. Check both agents (notifications/SendMessage); ff-merge their branches;
   for coven: bump bae pin, rebuild mac (signed, via LaunchAgent) + phone,
   remeasure publish gap (grep bae-app.log for "Drained blob uploads" /
   "Published Store writes" / "Sync stage timings").
2. User reproduces stuck-album on phone → read logcat "detail state" line →
   fix at owning layer → remove the diagnostic log line (uncommitted).
3. Continue journey plan: Storage Manager entrypoint run, then
   failure/pause/cancel/restart battery.
4. Queue consolidation backlog to coven agent (see Agents section).

## Found on resume (2026-08-19 ~02:00)

- Live panic class #2, diagnosed: upload_observer.rs:155 "preparation progress
  regressed" (log 03:17, during disk-full import). Cause: coven has two
  drainers of one upload queue — the sync cycle's drain and the host's explicit
  drain_uploads (bae calls it in manager/locality.rs:44 and storage.rs:99).
  Both admit the same Pending entry concurrently (documented in coven
  handle.rs:525 as benign for counts); each runs a full BlobUploadAttempt, and
  their interleaved preparation callbacks on one UploadBlobKey regress the
  bytes. The compare-and-set dedups only the Prepared handoff, after the loser
  already streamed a duplicate full prepare (duplicate encrypt + staging
  write). Fix (coven): serialize drain_uploads per store so one entry can
  never be in two attempts — removes the panic state and the duplicate work.
  bae's assert stays (it is correct once admission is exclusive). QUEUED for
  the coven agent after its current branches land.

## Resume session log (2026-08-19 02:00–03:00 local)

Landed since compaction (all pushed):
- bae main: 8aa50590 iOS reconnect wiring (finished the abandoned agent's
  diff myself; sim build verified), 2e88dfdc pin coven 5822f740, 11d24efc
  screenshots fix (48b6690d had mounted ArtworkLoadingBanner without
  injecting its store into PreviewScenes.libraryEmpty — Screenshots CI red
  since 08-17 ~18:00; verified locally via gui/501 LaunchAgent, all 7 scenes
  captured), 5a205751 pin coven 575c8141.
- coven main = 575c8141: retained-history durable reuse (750f209a),
  publish stage timings (5822f740), upload drain permit (575c8141 — fixes
  the upload_observer.rs:155 regression panic: cycle drain and host drain
  both admitted the same Pending entry; full analysis in the agent's
  branch report). Full workspace suite verified green on merged main
  (exit 0, 27 binaries).
- Agent now on the consolidation backlog, starting with duplicate ack
  caches + a two-device fixture for the live gap below.

OPEN — retained-history fix does not engage live: on the real GCS store,
prepare retained history measured 119s/35s/139s/192s per cycle with
174→177 durable rows (~700ms/commit, provider-latency-shaped) after the
fix. Fixture (single device) shows flat 27 reads. Suspect: per-commit
provider reads the single-device fixture can't see — prime candidate the
acknowledgement proof-chain walk (load_store_ack_predecessor consults no
cache). Agent instructed to find + close with a two-device+acks fixture.
Live smithy instrumentation (RUST_LOG now includes
aws_smithy_runtime=debug,aws_sdk_s3=debug in run-bae-logged.sh) is in
place but BLOCKED: see next item.

BLOCKED on user unlock — app booted locked at 06:28:54Z: kickstart while
the screen was locked → data-protection keychain refuses the master-key
read → bootstrap reads CloudHomeKeyState::Locked and silently skips sync
attach (no cycles at all since). NEW PITFALL: never kickstart
fm.bae.logged while the screen is locked; check `pmset -g log | grep
"Display is turned"` first. bae fix: bootstrap now warns on a locked
boot (app.rs, commit pending build check). When the user unlocks:
restart the app, watch for "store sync connected", then read the smithy
GetObject lines inside the next prepare-retained-history window and send
the object kinds to the coven agent.

Also pending: user still afk — phone off adb (Android first-tap bug
waits), Storage Manager journey run waits for a syncing app.

- PITFALL (cost a debugging spiral twice): never test app launches while the
  display is asleep or the screen locked. Sleeping display = no window
  presentation, parked keychain reads, boots that look "broken". Check
  `screencapture` ground truth or `pmset -g log | grep "Display is turned"`
  BEFORE diagnosing a launch. Related: never launch the app by name
  (`open -a bae` / AppleScript "bae") — only by full derivedData path; and
  never via launchd exec for interactive use (no LS registration, no windows,
  duplicate-instance magnet). File logs now in ~/.bae/logs/.

## Sync-efficiency mandate (2026-08-19 ~13:15, user-issued)

"Fix this implementation: redesign whatever needs redesign, remove as much
as possible, add as little as possible (but add enough), make it as
efficient as possible. Greenfield; wipe ~/.bae when done." Dispatched to the
coven agent as architect+implementer; design doc first
(coven plans/sync-efficiency-redesign.md), then serial branches.
Measured diseases driving it: canonical_input quadratic rows (223MB/385
commits; phone materialize 245s of a 391s pull), idle ack commits growing
history every 30s per device, serial fetch shapes in pull (blobs 80s),
per-cycle verifier rebuild duplicating durable knowledge. Live validation
protocol: merge → pin → rebuild mac+phone → wipe ~/.bae → re-pair → measure
(cycle stage timings + DB size + settled-cycle read counts).

## Self-serve validation harness (working)

- MCP automation: scratchpad/mcp.py (list | schema <tool> | call <tool>
  '<json>'); token in scratchpad/mcp-token (from Settings > Automation >
  copy-token via UI scripting); results land in scratchpad/mcp-last.json.
  Import pipeline proven end-to-end: import_candidates_list -> import_search
  -> import_release_prefetch -> import_start (storage_mode local|remote,
  pin, identity_choice exact). 146 valid unadded candidates on the SMB share
  = load corpus. Automation index seed bug fixed (commit on main) — surface
  survives mid-scan startup now.
- Measurement: ~/.bae/logs/bae.log.YYYY-MM-DD (file logging always on now);
  stage timings at INFO. sync_bench example for headless cycle runs.
  Fixture of the quadratic-era library: ~/bae-fixtures/library-quadratic-2026-08-19.
- Second device for re-pair validation: user's phone (away for now) or the
  Android emulator (~/.android/avd — check inventory when needed).

## FK-closure wedge (found ~19:30Z, fix in flight)

Live store wedged: every pull fails "retained Merge replay has an
unresolved foreign-key dependency". Agent's replay-from-zero dissection:
the sharing gate keeps a row via ONE selected gate-parent FK, and works
gated_by_descendants keeps only works with track_works/work_artists —
container works (work_parts parents) have neither, so all 8 parents are
structurally excluded while their 46 work_parts children ship. 100%
deterministic; publication order irrelevant; snapshot restore carries the
same hole. Fix (in progress, own branch): split kept (seeding) from
shared = closure(kept) over ALL FKs at reemit_subtrees /
partition_outbound / snapshot keep_clause; retract mirrors closure exit;
ALWAYS-ON FK-closure check at the publication boundary. Also real but
separate: cycle runs pull before publish-pending, so any pull failure
starves publication (livelock shape). Surfacing fixed on bae side
(sync-loop faults now logged; UI still flattens to "Something went
wrong" — core.error.category.internal — recorded, not yet fixed).
Live prefix lwerkjvwe/ stays untouched as repro until fix lands, then
milestone wipe + scripted re-import.

Second device ready: Android emulator AVD "bae-test" (arm64, android-36.1,
hand-written config in ~/.android/avd), bae APK installs and launches.
Throughput-race fix landed (d1ae3a7f): timestamps under the lock.

## Milestone runbook (redesign validation, ready to run)

Coven main = 2b9d9261: O(commit) rows (fcec5ae8), FK-closed shared set +
write-transaction closure check (bcc08465), acknowledge-on-change
(2b9d9261 tip). bae pin bump in flight. Steps:
1. Push pin, rebuild mac signed + android APK.
2. Preserve nothing further (fixture already at ~/bae-fixtures/...).
3. Wipe: rm -rf ~/.bae; emulator: adb -s emulator-5554 shell pm clear
   fm.bae.app (boot AVD bae-test first).
4. Launch app (open <derivedData path> — by path, never by name), create
   library, configure S3: bucket bae-import-dima, region us-east1,
   endpoint https://storage.googleapis.com, fresh prefix.
   BLOCKED INPUT: full S3 access key (GOOG1ENARDVFDSGBLHNNQYUCZ44FCLGSP2
   ZTMW3TLBSXE5TDDFMRSMM4NPW… tail clipped in every screenshot; secret
   known). Ask the user for bae-import-config.txt or the key tail.
5. Pair emulator via restore code (Settings > Connect another device;
   drive emulator UI over adb).
6. Scripted imports via scratchpad/mcp.py (token survives in keyring;
   candidates re-index on scan) — rebuild ~12 releases.
7. Measure against the frozen fixture: cycle stage timings, settled-cycle
   commits appended (target 0), DB size + retained bytes/row (target
   single-digit MB / ~7KB flat), publish total (target ~1-2s at limit 8),
   phone/emulator pull times.
