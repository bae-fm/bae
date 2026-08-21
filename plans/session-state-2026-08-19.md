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

## Pairing + publication findings (2026-08-19 ~23:30Z)

Live library: rollin-kendrick (faea0b82), prefix `bae-poppin-nina-db448je`
(user-typed; NOT bae-redesign-0819 — that prefix holds only a probe file).
7 releases imported via scripted MCP batch; 5 moved to cloud unpinned.

**Defect 1 — device join impossible between snapshots (blocks pairing).**
Emulator pairing choreography works end to end (copy code → adb inject →
Join → fingerprints matched 14b484e6 → mac approved, device now listed as
Member) but bootstrap install fails loud on the joiner:
`device join bootstrap cannot advance over unmaterialized row data at
306d7293…/2`. Cause: install_device_join_bootstrap installs plan commits
bare (verify(..., None, &[], None)); guard refuses package-bearing commits
not covered by a snapshot; only snapshot is generation 0 with coverage {}
(cadence: 100 changesets / 24h). Every join between snapshots fails.
Fix (briefed to coven-2c, priority over branches 3-6): plan builder fetches
packages for uncovered commits, install materializes rows for real via the
verify call's existing packages/package_application slots.
Emulator state: half-joined — Member on the store, no local materialized
store, joiner journal holds the attempt. After fix: retry Join; if the app
won't resume the attempt, trash the Member row on mac and re-pair.

**Defect 2 — publication re-downloads every referenced blob.**
"publish packages" stage: 13 blobs → 15.0s, 24 → 11.1s, 33 → 20.5s, while
creating ONE ~74KB package object. publish_prepared_remote_object RowBlob
arm calls verify_blob_object per blob = full GET + rehash of the encrypted
blob it just uploaded (~200-500MB re-downloaded per publication). Fix
(briefed to coven-2c, branch-5 territory): trust the durable Created record
for own uploads (HEAD at most); full-content verify only for foreign
objects, once, behind durable verified-objects. Snapshot publication has
the same disease (publish_store verify_blob_object per snapshot blob).
This answers the user's "publishing still seems to take quite a while".

Residue cleaned: live_probe_diagnostic module removed from coven
s3.rs; coven tree clean on main = 2b9d9261.

Emulator specifics: adb `input text` DOES take the full 324-char pairing
code (tap field first; parse renders Library/Provider rows when it lands).
Join button with keyboard up: (540,1318). Mac "Copy code": lclick -1203 521;
Approve: lclick -1164 464 (window at -1453,21,500,688).

## Agents in flight (2026-08-20 ~00:20Z refresh)

- coven-2c (peer session, tmux): 3 branches — (1) device-join bootstrap
  materializes uncovered package commits [blocks pairing], (2) publication
  stops full-GET-verifying own blobs [branch-5 territory], (3) coven-keys
  types errSecInteractionNotAllowed (-25308) as transient-refusal KeyError.
  Merges: they push, I rebase + --ff-only, then bump bae's coven pin.
- product-engineer (background): serial branches `probe-error-surfacing`
  (SyncSetupWizard error+Connect pinned outside scroller via
  ErrorDetailDisclosure; Avalonia syncStatus adjacent to connect) and
  `boot-failure-surfacing` (no stuck .loading, keyring-init failure still
  discovers, WelcomeChooseView surfaces section-load failures, KeychainService
  refusal ≠ absent, NativeBae.Init preserves category/detail). Audit each
  with code-review-auditor before merge.
- Parked for after coven pin bump (PR C): typed keychain-refusal end-to-end —
  bae BootstrapError arm + bridge category, opener refused-vs-absent outcome,
  UnlockView copy, screen-unlock/wake re-check observers.
- Post-fix measurements queued: Burning Spear b0bcf324 (95MB, local) is the
  clean publish sample; emulator re-join (Member 14b484e6 exists — trash +
  re-pair if the joiner journal won't resume) then initial-pull timing.

## Routing correction (2026-08-20 ~01:20Z)

coven-2c never received my briefs: cross-session messages from this session
are HELD at its permission gate ("permission mode class doesn't match",
needs manual approval in its tmux pane — one held status-ping is sitting
there now; deny it, the work moved). I did not touch that session's
permission prompt. All three coven branches now run under my own
product-engineer agent instead: device-join-bootstrap-packages (critical
path), trust-own-blob-uploads, keychain-transient-refusal.

bae side landed tonight (main = c05d84b5): probe-error-surfacing 340d9ad1,
boot-failure-surfacing a135aa98, display-line-optional-rendering da3ffec6,
import-error-localization c05d84b5 — all audited pre-merge. Engineer queue:
applehost-localization (CloudKit literals + gate extends to AppleHost,
fullest-edition policy), ios-catalog-backfill (145→full keys + ios.yml
gate), keychain-delete-fail-loud.

## Backlog from branch-5/6/7 work (2026-08-20 ~03:00Z)

- CloudKit recovery guidance (not-signed-in / quota / permission / zone-gone)
  currently localized Swift-side but rides BridgeError detail, whose contract
  says never-translated. Real shape: BridgeErrorCategory arms in bae-core so
  the guidance is a keyed headline. Also the copy says "System Settings →
  Apple ID" — macOS wording rendered verbatim on iOS; fix wording per
  platform when the category arm lands.
- iOS catalog divergences: 5 keys where iOS and macOS translations drifted
  as synonyms — left alone deliberately; unify if a register pass happens.
- Pitfall (in agent memory too): build-ios.sh needs ~30GB peak; SwiftPM
  caches the Package.swift binary-target-exists answer in
  ~/Library/Caches/org.swift.swiftpm/manifests — an xcodebuild run before
  build-ios.sh poisons later builds with ffi_bae_bridge link errors until
  that cache is cleared.

## Join milestone (2026-08-20 ~03:00Z local 22:57)

Emulator joined rollin-kendrick end-to-end on the fixed stack (bae 3bfb8201,
coven 2b6f055b): fresh pairing (old Member removed, key rotated), fingerprints
9bd967f7 matched, bootstrap materialized post-snapshot commits, library
renders all 5 albums. BUT: approval→library = 278s for 12 commits / ~2MB
real data. Join path has zero stage timings (nothing logs between master-key
load and "joined Store device"). Briefed coven engineer:
join-stage-timing-and-latency branch — instrument the whole choreography,
then fix what the numbers convict (suspects: fixed-interval exchange polls
on both sides, serial per-object GETs in bootstrap resolution).
Also found: Cmd+, opens a "bae Settings" window wired to a nil session ("No
library loaded" while library is open) while the menu's Settings… opens the
real one — macOS settings-scene wiring defect, unbriefed yet.

## Settings fix verified + doppelganger pitfall v2 (2026-08-20 ~03:30Z)

settings-scene-session (0e37efc9) live-verified: Cmd+, opens the populated
Library settings window backed by the open session; Devices lists Owner
7d13f65c + Member 9bd967f7 (the joined emulator).
Pitfall v2: agent worktree builds register their bae.app with LaunchServices;
`tell application "bae"` / open-by-name can LAUNCH the worktree copy (it did
— second instance from bae-wt-error-surfacing shared ~/.bae, failed its
library open with "store is already open" rendered in the NEW welcome error
UI — fail-loud worked as designed). Remedies applied: killed it,
`lsregister -u` + deleted the worktree app bundle. Rules: launch only by
full path; activate via System Events `tell process "bae"`, never
`tell application`; after any agent runs GUI tests, sweep for stray bae
processes (engineer left 3 running earlier).

## Publication measurement — fixed (2026-08-20 03:51Z)

Burning Spear b0bcf324 (95MB/19 files) moved to cloud unpinned on coven
8929a41 (trust-own-blob-uploads): blob drain ~18s (raw 95MB), then
"Store write publication" total 1457ms — publish packages 243ms (was
14990ms for 13 blobs; the full-GET re-verify is gone). Remaining >500ms
stage: authorize outbound 639ms. User's "publishing takes a while" is
answered and fixed. bae pin 764708bb.

## Key-rotation wedge + instrumented join run (2026-08-20 ~04:00-05:00Z)

- Snapshot wedge root cause (coven engineer, fix written, push pending
  disk-recovery): snapshot preflight + blob_preparation compare stored blob
  locators against a locator recomputed under the CURRENT key generation;
  any member-removal key rotation re-identifies every pre-rotation blob and
  wedges snapshot publication permanently. Fix: locator_is_this_rows_upload
  (content+reachability, not key fingerprint). Existing store unwedges on
  ship, no rebuild. My pin bump was coincident, not causal.
- Disk hit 100% (Bash could not even start); recovered via Monitor-shell
  rm of incremental dirs + reclaim.sh --force. Engineer's uncommitted fix
  survived in /Users/dima/dev/coven-wt-fixes.
- Instrumented join re-run (fresh pairing, fingerprints 2d7d21e4, approved
  04:42:01Z): owner-side stages now visible — first conviction:
  "activate same-provider device 68054ms" (one owner step, 68s). Joiner
  CPU-bound (115% CPU, ~3KB/s net) 20+ min in "Installing library
  snapshot…", stages print at phase end so still dark mid-phase. Suspect:
  full history verification (exclusion+rotation commits carry circle
  controls, disabling the fast path) under debug-build crypto on emulator.
  Watcher armed; numbers on completion.

## Join runs 2-3 evidence (2026-08-20 ~05:40Z)

Rotation fix live-verified: "Snapshot created and pushed local_seq=101"
first cycle after relaunch (pin f724e148, coven 8a90bea6). NOTE: stream
length is ~101 commits — the earlier "12" was retained rows, not history.
Run 2 (post-exclusion history): joiner CPU-bound 28min, silently abandoned
(pairing code expired mid-install; cancelled future never reports).
Run 3 (covering snapshot): owner "activate same-provider device" 61.8s
(run 2: 68.0s — reproducible, native mac hotspot #1); joiner died silently
139s post-approval, zero coven log lines even from the new instrumentation.
Join is likely BROKEN (not just slow) on stores with a rotation in history.
Engineer briefed: in-workspace repro (rotation fixture + fresh join),
activation-stage breakdown, report-on-drop hardening, typed abandonment.
Android app defects queued separately: startup ANR (PlaybackService waited
23s), silent join failure UX, pairing-code entry flakiness under ANR.

## Run 4: join works, fully instrumented (2026-08-20 06:20Z)

121s approval→library (was 278s / 28min-fail / 139s-fail). Breakdown:
waited-for-owner 69.4s (activation walk — fix join-activation-retained-reads
pushed: 532→14 reads, count-probe test committed); install history 18.6s
(pull O(n²) CPU, next fix); open cloud home 3.7s; download snapshot 2.7s;
row-data resolution 2.4s for 193 commits (fetch blobs 58ms — PackageBlobPolicy
fix live-confirmed; run 2's 28min was the join fetching every blob ever
bound). Anomalies queued: uncovered_commits=193 despite gen-1 snapshot
covering ~101 (bootstrap snapshot selection?); stream inflated 101→193 in
an hour of join/remove churn (~45 commits per cycle?).
Projected post-activation-fix join: ~30s, dominated by the quadratic.

## Run 5 (2026-08-20 06:42Z): 74s

Join now 74s approval→library (278→121→74 across fixes). Breakdown:
owner activation 14637ms (authorize writer 2919, provider access 7,
activate the join 11703 — next decomposition target); install history
13662ms @ 195 uncovered (snapshot-selection branch pending); open cloud
home 5481ms; snapshot download 2308ms; row data 1712ms. Merged tonight to
bae main additionally: surface-join-abandonment 1e59034b (typed DeviceJoin
failures, 31 locales, journal-resume bug fixed), fix-android-ktlint
21f73754 (main's Android gate was red). ANR investigation refuted with
measurements: host starvation + first-run JIT, not app startup — quiet-host
protocol now applies to all measurement runs.

## Run 6 (07:43Z): regression signal — coverage fix ineffective live

98s join, uncovered_commits=197 (zero credit) with bbcc6f18 live on both
sides; install history 24.5s, owner activate-the-join 13.5s. In-workspace
test passes ⇒ divergence upstream of the crediting walk. Hypotheses ranked
to engineer: (1) select_maximal_stable_store_snapshot rejects gen-1 on a
store with 5 exclusions → falls back to gen-0 empty coverage; (2) live
gen-1 image (captured by pre-fix 8a90bea code) doesn't carry coverage in
the shape the walk reads (greenfield: fresh snapshot over compat);
(3) depth/stream-count assumption. Requested: selection-time log line
naming chosen generation + coverage tips (selection currently invisible).

## Run 6 root cause + design decision (08:0xZ)

Gen-1 snapshot is PERMANENTLY unstable: build_snapshot_stability demands an
ack from every device active at the snapshot's coverage, acks only ride
commits, and two joined-and-idle pairing-test devices never authored one —
so bootstrap (which reused reclaim's stability predicate) always fell back
to gen-0 (coverage {}). Reproduced in-workspace in 0.16s. Hypothesis 2
(image compat) affirmatively dead: install repopulates snapshot_coverage
from signed meta, old images fine, no shim. DESIGN DECISION (mine): split
the predicate — bootstrap selects maximal snapshot passing
verify_snapshot_authority (signed meta), reclaim keeps unanimity. Safety
property named: post-install verification strength unchanged; baseline
seeds from signed meta only. Follow-up branch: reclaim-unanimity-membership
(membership change before unanimous ack permanently blocks reclaim;
excluded devices must not count). Selection-time logging landed (a08dd37e).

## Run 7 attempt (08:56Z): blocked on stale wire-shape

Predicate split merged (coven 3b33e72c, bae pin + both apps rebuilt; audit
confirmed safety independently — ack chains were never verification
machinery). Run 7 failed at owner approve: "unknown field `stability`" —
the approve path deserializes a stale old-shape artifact (cloud exchange
slot or local pairing journal from runs ≤6). New failure surfacing worked
on both sides (mac dialog line, joiner returned to form). Engineer
diagnosing: stale-purge vs unscoped-attempt-read (the latter is a real
defect — concurrent pairings would collide identically). New selection
logging proved itself: reclaim's gen-1 rejection (idle devices) now
visible per cycle.

## Run 8 (09:34Z): coverage crediting verified live; residue is coverage-blind

146s total. WINS: uncovered 10/199 (was 199/199), row-data resolution 51ms
(was 2400ms), gen-1 selected for join with tips logged while reclaim's
unanimity rejection logs separately — the split behaves exactly as
designed. RESIDUE (all coverage-blind, engineer briefed): install history
18.2s @ only 10 uncovered (per-commit verification walk over all 199
ignores coverage — is it redundant with the snapshot's signed authority?);
install snapshot 15.2s (gen-1 real image vs 0.7-0.9s for near-empty gen-0);
owner activate-the-join 17.2s (grows with depth). Confound: every test
cycle inflates the stream — reclaim-unanimity-membership promoted (bounded
depth = the real fix for store lifetime). Journal purge done (14 rows).

## Relay handoff #2 (2026-08-20 ~10:10Z)

Second coven engineer retired clean (all merged: coverage walk, fixture
helper, selection logging, predicate split, journal fault isolation, walk
timing; coven main = d142f8cc). Third engineer spawned with queue:
1) trim-carried-closure (approved: owner carries closure from snapshot
coverage forward; bootstrap_cut + predecessor check untouched; red-first:
both-side counts scale with tail, tamper-negative, artifact/journal size
bounded by tail) — kills joiner's 199-commit verify walk, half the owner
activation, the 6.7MB journal rows;
2) stage-and-cut-install (15.2s image install; owner-boundary gate caution
recorded); 3) reclaim-unanimity-membership (eligible-set design first);
4+) journal residual, pull quadratic (cross-provider), open_stream
verification flag, trust-own-circle-blobs.
bae side: all merged through 1e59034b + ktlint 21f73754; pins current at
962c85b7 era (next bump after trim lands). Run 8 = latest live numbers.

## Run 9 (10:40Z): trim verified; three 15-18s lumps remain

118s. Trim live-confirmed: plan 12 commits (was 199), carried verify 145ms,
carry 8ms. Remaining (branch 2, briefed with bounds): joiner install
snapshot 15.9s; joiner install history 17.1s (proven NOT carry/rows/prep);
owner activate-the-join 18.3s of which plan-building only 2.8s. Also open
cloud home ~7s recurring. Reclaim design doc d4ae2f36 approved (eligible
set = active-now ∩ active-at-coverage, each conjunct argued; two new
flagged shapes queued as separate investigations).

## Run 10 (11:20Z): full cost map, cuts commissioned

123s, plan 14/14. Joiner: membership chain walked TWICE over network
(install owner membership 16.4s + open cloud home 7.4s, sequential);
install-the-snapshot 12.1s = three full-image passes (unindexed
max-updated-at scan, id-materializing validation, FK check). Owner:
seed retained history 10.6s (the 4cddd186 fix re-verifies all 200+
retained rows per activation for a 14-commit plan — trim one level down);
authorize writer 5.6s unstaged; commit plan 3.4s; activate upload 3.3s.
Cuts commissioned with acceptance targets: install history <2s, install
snapshot <3s, owner activation <5s. Trim wins visible (carried history
materialize 1.0s, row data 42ms).

## Run 11 (12:20Z): 93s; final lump convicted; engineer relay #3→#4

Walk-once memo delivered both sides: owner activation 26.1→9.7s (seed
10.6→3.6s, plan walk 1551→46ms), joiner wait 31→16.5s, install history
18.6→11.4s (owner membership 16.4→9.0s). Convicted the last monster:
"Retained replay baseline capture" 11916ms inside snapshot install
(validate the image 4697ms + ~6s unstaged; re-validates and stores a copy
of the just-verified image; read-back re-proves content-address). Engineer
#3 retired clean (branches membership-walk-once + stage-snapshot-install
merged; disproved the three-passes hypothesis with 200k-row measurements).
Engineer #4 spawned: LIST-then-fetch membership (user-directed design),
trim-baseline-capture, seed-only-the-walk, reclaim implementation
(approved doc d4ae2f36 merges only with its code), then HLC-in-txn,
pull quadratic, open_stream, circle blobs, journal residual.
Join trajectory: impossible → 278 → 121 → 74 → 146* → 118 → 123* → 93
(* = store grew / measurement runs). bae pin: cf0f67b1 era.

## Run 12 (13:46Z): 98s; cuts landed, variance-bound

Per-stage: baseline capture 11.9→5.8s (validate-the-image 4.2s now
dominates, FK scan inside), install snapshot 14.6→8.5s, owner membership
9.0→6.2s (LIST cut), authorize writer 3.8→0.9s, owner activation →8.1s.
Wall clock flat vs run 11 (93→98) — remaining spend is variance + raw
transfer + the ~210-commit churn depth. Engineer #4 mid-branch-4 (reclaim
implementation; both judgment calls ratified: dominate-current-requirement
at execute time; empty-set hold documented as examined conservatism).
Endgame after branch 4: WIPE ~/.bae per mandate, fresh store, final
scorecard (import → move → join) on the finished stack.

## STANDING GOAL (set 2026-08-20 ~15:00Z)

1. Live store compacts (membership-conjunct fix → gen-1 selected → depth
   falls). 2. Join fits budget: single-digit seconds, ~20 requests, proven
   by live request-count lines. 3. Finale: wipe ~/.bae, fresh library,
   record full journey (import → publish 1.5s ✓ → pair → join → stream).
4. Nothing unmerged; state doc closes with the scorecard.

## Key finding (15:00Z): member removal ≠ device exclusion

Live store shows ALL 13 devices of 11 removed members still `active` in
all 208 device states — remove_member ends grants + rotates keys, never
touches device state; StoreDeviceStatus::Inactive is separate choreography
(retiring a lost device of a member in good standing). Reclaim's eligible
set now requires: active at coverage ∩ active now ∩ is_member_now (new
MembershipChain::is_member_now; can_write_now wrong for read-only members).
Fourth red test with a genuine second member (admit_and_activate_peer →
remove). In gate; push pending.

## Finale runbook (execute after goals 1-2)

1. Quit bae; adb uninstall fm.bae.app. rm -rf ~/.bae. Optionally purge old
   cloud prefixes (keep bae-poppin-nina-db448je as pre-compaction relic? NO
   — greenfield: delete bae-poppin-nina-db448je + lwerkjvwe + bae-redesign-0819
   via gcloud AFTER the scorecard, keep during in case of comparison).
2. Launch bae (full derivedData path only). Welcome → Create new library.
   NEW prefix: bae-final-<date>. S3 form: bucket bae-import-dima, region
   us-east1, endpoint https://storage.googleapis.com, creds from
   scratchpad/s3-creds. Probe error now renders next to Connect (fixed) —
   verify field-by-field with screenshots before Connect; the form-scroll
   mangling pitfall from 08-19 applies.
3. Scripted imports via mcp.py/batch_import.py (SMB share folders; token:
   Settings → Automation → Copy token if regenerated). 3-5 releases.
4. move_to_cloud unpinned per release via release_storage_action; record
   publication stage lines (expect ~1.5s each).
5. Pair emulator (fresh pm cleared app): full choreography, record both
   sides' stage+request lines. Target: single-digit seconds.
6. Stream a track on the emulator (dumpsys media_session PLAYING).
7. Write scorecard section; update plans/cloud-upload-user-journey.md
   items; final commit+push.

## Relay #4→#5 (16:0xZ)

Engineer 4 ran out of context MID-WORK: the member-conjunct fix
(is_member_now third conjunct) + request-counter start left UNCOMMITTED in
coven-wt-relay4 (336 insertions/16 files; diff preserved at scratchpad/
relay4-uncommitted.diff). PR C merged meanwhile (bae fd5fa73a): keychain
refusal typed on both routes, KeychainLockedView, NSWorkspace wake/unlock
retries, iOS scenePhase retry, Avalonia gap documented in-code. Engineer 5
spawned: (1) reclaim-member-conjunct from the inherited diff [gates live
compaction], (2) stage-request-counts [gates join budget proof], then
membership rollup, laggard snapshot catch-up (+ veto-lapse policy question
for the user), snapshot-download serial GETs, install-history remainder,
handshake RTT audit, seed memo, parked items. Baseline for compaction
verdict: 208 retained commits, 87MB store.db, 213 cloud candidate objects.

## Compaction verdict metrics (pre-fix baseline, 16:30Z)

1. Retained commits: 208. 2. store.db: 87 MB. 3. Cloud candidate objects:
213. 4. NEW — idle cycle cost climbed with depth all night: 1.7s (00:00)
→ 15.9s (11:00) → ~11s now; some per-cycle stage walks full history.
Post-compaction expectation: cycles return toward ~1.5s. If they stay
elevated after depth falls, the per-cycle walk is the next named cut
(candidate-stability evaluation re-verifying unchanged state every 30s is
the prime suspect).

## Watch item (17:4xZ): checkpoint-state mismatch on version-skewed member

Emulator (coven 5e442ab6-era APK) pull fails loudly: "snapshot Merge
checkpoint state differs from its signed reference" against the mac on
ce0d32c6 which has published newer snapshots. Presumed version skew
(greenfield: mixed versions unsupported; loud refusal correct). MUST
RE-VERIFY on matched builds after the conjunct merge + rebuild + fresh
rejoin — if it reproduces same-version, it is a real state-derivation
defect (top of coven queue with this exact error line). Also: emulator
networking goes stale after long uptime (os error 103 connect aborts) —
adb reboot fixes; wifi toggle does not. The joined device's failure
banner + Retry surfaced everything correctly.

## Correction (19:1xZ): gen-1 "selection" was ambiguous; decline made visible

The "Selected the Store snapshot generation=1" line is shared by BOTH
selectors (installable=join, acknowledged=reclaim) with no discriminator —
so goal-1's "gen-1 selected for reclaim" is UNPROVEN; may have been the
join selector. Engineer 6 falsified both decline suspects against the live
DB (store_reclaim_operations EMPTY — no stale block; supersedes-seed is
Circle-only) and found the swallow: mod.rs:290-292 converts NoSnapshot/
MissingAcknowledgement to Vec::new() with no log so Store trouble can't
block Circle reclaim. Standing suspicion: gen-1 coverage-era devices
include 2 never-acked (4401619a, 64a75c2d) — excused only if their
principals read as non-members via the cloud-walked chain; the new
per-cycle StorePackageReclaimReport (coverage-or-typed-reason +
considered/retained/authorized counts + selector discriminator) settles it
next live cycle. Latent hazard queued: completed reclaim operations are
never deleted → permanent per-target block. stage-request-counts pushed
(c4c90108), under audit.

## Goal-1 final gate NAMED (20:5xZ): owner baseline never advances

New instrumentation live (b4347329 both apps). PROVEN: selector=
"acknowledged" generation=1 — reclaim selects gen-1 (conjunct fix works).
Decline named by the first-ever per-cycle report: considered=5
retained_for_replay=5 authorized=0 — all five package targets behind gen-1
pinned by the OWNER'S OWN replay retention; the owner's baseline is still
Genesis (standing devices never re-baseline; only joiners get modern
baselines). This is what holds 208 retained rows / 87MB / the depth curve.
Engineer 7 spawned: advance-replay-baseline (atomic advance over an
acknowledged-stable snapshot + retained-row retirement; red-first on the
report counters). Request counters also live on both sides now.

## Request counters LIVE (21:5xZ) — first readings

Cycle line now carries total_requests + per-stage /Nreq. First reading,
IDLE cycle on the 210-commit store: total_requests=107 (pull 56req/6.6s,
refresh authorization 12req, collect tombstones 1req...). Named finding:
idle cycles are ~20-50x over a sane request budget (~300k req/day at
idle); expect partial collapse post-compaction (pull's 56 scale with
depth), remainder to be cut by stage. Gen-1 selection for reclaim was
PROVEN earlier via selector="acknowledged" discriminator. Baseline
advancement (engineer 7) mid-gate; Pixel + emulator now genuinely on
b4347329 (previous wave's Android bridge failure was pipe-masked — .so
was 3 revs stale; caught by the user questioning the APK timestamp).

## FIRST COUNTED JOIN — real hardware (Pixel, 22:2xZ)

User re-paired their Pixel (fresh data) live: "Same-provider device join"
total 12369ms / 78 requests. Breakdown: open Store storage 2456ms/61req
(membership chain = 78% of requests — rollup fix kills it), download
snapshot 1902ms/1req, install snapshot 2069ms/0req, install history
5805ms/16req (owner membership 1880ms/1req — memo working), 31 uncovered
commits' row data 1493ms/10req. From 28-minute failure this morning to
12.4s. Budget (single-digit s, ~20 req): time at the doorstep, requests
4x over with one named cause. Devices list: 693559f7 (emulator),
owner, + phone identities b6b6fc0c/b9607a28 — ONE is the fresh pair, one
is pre-reset dead weight; identify before removing (ask user or check
which acks).

## BROADENED MANDATE (user, 22:4xZ, stepping away): sync at scale

"make it so syncing larger libraries with many things added doesn't suck."
Campaign order:
1. Land advance-replay-baseline (engineer 8, in flight) → live compaction
   → depth bounded forever (goal 1 closes; cycle costs partially collapse).
2. Cycle request budget: idle cycle must be O(streams) requests (~2-5),
   not 107 (mac) / 680+77s (phone measured live). Stage the pull's 56req,
   cut. This is the phone-battery/latency number.
3. Bulk-add catch-up: measure a device catching up after many releases
   land at once (emulator; user took the Pixel). The catch-up path must be
   O(new items) with fanned-out fetches, not per-commit serial. Includes
   the laggard-catch-up-via-snapshot design (device far behind re-
   bootstraps instead of replaying).
4. Membership rollup (61 of 78 join requests) → join inside budget.
5. externalize-baseline-payloads (user directive, queued on engineer 8).
6. Finale on a fresh library incl. a BULK import wave as the scale test.
Phone gone; emulator is the second device for all measurements.

## Second counted join (22:0xZ): emulator re-pair confirms the profile

Emulator (fresh, 5d259665): join 13711ms/82req — matches Pixel
(12369ms/78req). Membership chain = ~65req/~80% both devices; snapshot
download 1req; wall approval→library 45s (owner work + exchange waits on
top of the 13.7s join phase). Devices list now: owner + phone (fresh) +
emulator (fresh) + one stale phone identity (b6b6fc0c or b9607a28 —
identify before removing). Join profile is characterized and stable;
membership rollup is the single named cut to budget.

## Design decision (22:5xZ): covered positions resolve to coverage

Engineer 8 (retired at 16 failures, from 49; four acceptance tests green;
two pre-existing holes fixed: images shipped without circle_control
activation refs; replay double-applied covered rows) surfaced the deep
question: what is a checkpoint for a covered-but-not-tip position after
retirement? DECISION: the snapshot's signature is the authority for its
entire covered prefix — covered-position queries resolve to coverage
state; cut checks treat covered as satisfied-by-coverage; summaries over
covered prefixes compose from coverage. Per-position states below
coverage cease to exist (that is what compaction means). Old per-position
published artifacts unsupported (greenfield; endgame wipes). Engineer 9
implementing + updating the 13 pre-fix-behavior tests with rewritten docs.
Underlying shape named for the record: retained_merge_materializations
conflated replay-input with retained-authority; the baseline work is
un-conflating them.

## Relay pass 9→10 (23:4xZ): four failures left, three rulings issued

Engineer 9 (16→4): implemented covered-resolves-to-coverage cheaply (the
feared 4 summary tests didn't break); found+fixed two more holes — device
join closure ran to GENESIS demanding packages reclaim deletes (dormant
join wedge, preempted; the "no snapshot installed" comment was false),
and the announcement chain now resumes at the snapshot's signed
announcement_frontier instead of re-walking from the anchor. Blind alley
documented (frontier-row retention breaks the acceptance signal).
RULINGS to engineer 10: (1) own-ack licenses own baseline advance
(unanimity licenses cloud deletion only; prefer advance-then-ack ordering
to make the deletion window unrepresentable); (2) snapshot verification
stops at the installed baseline (kills pull's sum(1..N) covered re-reads);
(3) restore assertion flips (retention proven load-bearing). Then:
workspace green, gate, push, live acceptance on the counters.

## Advance falsified live (03:5xZ): tests green, mechanism inert

67ec28af live on all devices. Triggered a real store write (metadata
edit; publication confirmed) forcing a fresh ack — across 6+ min of
cycles: no advance log line, counters frozen at retained_for_replay=5.
Both trigger paths dead live (standing-ack never re-enters staging;
fresh-ack staged without advancing). Directive to engineer 10: instrument
the advance decision (advanced/declined-with-reason per staging), then
move the advance to cycle level licensed by any standing own-ack naming
an acknowledged-stable snapshot with baseline below coverage; red-first
fixture must have a PRE-EXISTING standing ack. Side finding: publish
acknowledgements = 1.7-2.5s/18req every quiet cycle (idle-budget list).

## GOAL 1 ACCEPTANCE (08-21): live compaction fired

On coven 8db630fd ("advance licensed by newest snapshot-naming ack in own
retained history"), mac relaunch, first cycle: "Advanced the replay
baseline over an acknowledged snapshot commits=189 pins=119". Next cycle
reclaim report: considered=5 retained_for_replay=0 already_authorized=0
authorized=5 packages=5 copies=5 — five Store packages + copies deleted
from the cloud. retained_merge_materializations 208 → 46. Depth falls,
gen-1 selected, pins released: goal 1's mechanism is closed.

Caveats recorded honestly:
- store.db 129.6 MB (GREW — the new baseline image is stored in-db;
  externalize-baseline-payloads 11322c80 is on main but this store
  predates it / vacuum never ran; endgame wipe resolves, or vacuum).
- Post-compaction cycles DO NOT settle: pinned at ~39s / exactly 457
  requests per cycle. Stage line convicts: "reclaim packages
  31862ms/391req" EVERY cycle — with nothing left to reclaim, the reclaim
  leg re-runs its full provider-side evaluation every 30s. Also: advance
  stage 3req at steady state (should be 0, local-only decline), refresh
  authorization 13req, publish acknowledgements 16req, pull 33req (down
  from 56 — depth drop helped). Engineer 10 dispatched: reclaim
  re-evaluates only when inputs change (new snapshot/ack/membership/
  package-bearing commit), typed "inputs unchanged" decline, 0 requests
  on the quiet path; red-first counting-home test asserting a settled
  store's full cycle issues an exact small request count.

## Stale phone identity removed (08-21 04:46Z)

Identified by cloud timestamps: join attempt 3fc54cd6 (member b6b6fc0c)
completed 21:10:12Z with its device 13ccedc4's last ack 21:10:27Z — the
pre-reset pairing, dead from the moment the user wiped app data. Attempt
faf034aa (member b9607a28) at 21:51:15Z is the live re-pair ("i paired
already"), device adfa42b1 acking since. Removed b6b6fc0c via Settings →
Library devices row (AX-verified row position, confirm sheet "Remove this
device? … key will be rotated"); rotation logged 04:46:15Z; devices list
now owner + 5d259665 (emulator) + b9607a28 (Pixel). Rationale beyond
hygiene: a dead member that never acks is a permanent unanimity laggard —
it would block cloud reclaim past its last acknowledgement forever.
Watch item: next snapshot/ack round after this membership churn must
still license advances (the 8db630fd fix reads the newest snapshot-naming
ack from own retained history, so churn shouldn't stall it — verify live).

## Quiet-path fix verified live (08-21 05:2xZ) — GOAL 1 CLOSED AS A LOOP

coven 7c359a48 (bae pin 78090a14): settled cycles collapse 458req/39s →
48req/5.4s ("inputs unchanged" typed decline visible). Better: the whole
compaction loop cycled unattended on the new build — gen-2 snapshot
published (cycle: publish snapshots 186req/23s), acknowledged (next
cycle: publish acknowledgements 235req/20s), baseline advanced again
(05:21:48Z), retained rows 208 → 46 → 1. Depth falls generation over
generation with no operator involvement. Zero warns/errors.

Steady 48 = pull 33 + refresh authorization 14 + tombstones 1. Change-
path convictions (once per generation, not quiet-path): snapshot publish
186req and its acknowledgement 235req — both should be O(members +
image), look like history walks. Ack-path fix dispatched (disjoint from
rollup work); snapshot-publish fix queued behind engineer 11's rollup
(both touch snapshots/publication.rs).

## Found: store.db's 125MB is a no-op write journal (08-21 05:4xZ)

dbstat on the live store: store_writes 54.7MB (81,887 rows, status
"local_only") + payload_owners 9.8MB/82k rows + its 11MB index +
protocol_state 28.9MB. The 82k rows are EMPTY writes: affected_rows=[],
changeset_hash = hash-of-nothing, blobs=[], and store_write_partitions
holds 20 rows total — so none of them carries anything replayable, yet
retained replay loads all of them as overlays and nothing ever deletes
them. Writer identified live (sample + rate measurement, ~1-3 rows/s):
playback position persist at 1Hz (runtime.rs:538) — music has been
playing on this mac for days; every persist journals a permanent empty
row + a payload_owners row for the empty hash. Growth ~15-20MB/day idle-
while-playing; replay cost grows with it. Engineer 12 dispatched:
no-partition captures leave no rows; LocalOnly journal bounded across
baseline advances; red-first with exact row-count assertions. bae's 1Hz
persist itself stays (crash-safe resume is a product choice; the journal
was the defect). protocol_state 28.9MB not yet explained — next look.

Addendum — protocol_state explained: 30 rows, all device_join/<attempt>/
progress records, largest 8.7MB, ~28MB total. Every join attempt ever
made (completed, expired, superseded — 16 visible in cloud listings)
keeps its full nested progress envelope forever, and the matching cloud
objects (device-join-attempts/, -outcomes/, -transport/) linger too.
QUEUED: terminal join attempts must delete their durable progress row
and their cloud transport artifacts as part of reaching the terminal
state (atomic with it, not swept later). Assign when an engineer frees.
