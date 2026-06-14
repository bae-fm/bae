# Bridge execution model

How `bae-bridge` runs work for the native apps, and which thread each call's
work — and stack — lands on.

## The constraint

`bae-bridge` is a uniffi library; the native apps (Swift, Kotlin, …) call its
exported functions across the FFI. uniffi drives an exported function on **the
foreign caller's thread**:

- A **synchronous** export (`pub fn`) runs directly on the calling thread.
- An **`async`** export (`pub async fn`) is polled by uniffi's
  `rust_future_poll`, which calls `Future::poll` **on whatever thread drives
  it** — for Swift, a cooperative-pool thread. uniffi does *not* hand the future
  to a runtime worker; it polls it inline.

So either way the work, and its **stack**, lives on the foreign caller's thread
unless we move it. Swift's cooperative pool hands out ~0.5 MB stacks. Most work
fits. The AWS-SDK S3 future chain (coven's `CloudHome` operations) does not: its
endpoint / auth-scheme resolution descends ~30 nested futures *synchronously* in
a single `poll`, well past 0.5 MB in debug builds (where async state machines
aren't collapsed). Polling it on the caller overflows the stack — a SIGBUS,
"Thread stack size exceeded".

bae's tokio runtime (built in `bridge::init`) gives its **worker threads 16 MB
stacks** for exactly this reason — the sync cycle's snapshot/apply futures are
just as deep. The fix for any deep call is to run it on a worker (or, where that
is impossible, on a comparably sized dedicated thread).

## What the apps call, by weight

- **Shallow / local** — album & release reads, storage pages, playback & queue
  control, config/token reads, file & image paths, the cloud-outbox queue
  (read / retry / cancel), restore-code encode/decode (`generate_restore_code`,
  `decode_restore_code`). Local SQLite or in-memory, no network call; cannot
  descend deep enough to overflow.
- **Deep / network** — the cloud operations: `save_sync_config`,
  `sign_in_cloud_provider`, `use_cloudkit`; and the pre-`AppHandle` onboarding
  functions `restore_from_cloud`, `restore_from_code`, `oauth_authorize`,
  `oauth_complete`. These reach coven's `CloudHome` and run the deep future.
- **Streaming / fire-and-forget** — `subscribe_ui_events` (callback stream),
  `trigger_sync`.

## How each is run

Three cases, decided by depth and by whether the future is `Send`:

1. **Shallow / local → sync export, `self.runtime.block_on(fut)` on the caller.**
   Local and can't overflow, so there's nothing to move. Several of these also
   feed *synchronous* SwiftUI rendering (e.g. `image_path` / `file_path` feeding
   `NSImage(contentsOfFile:)`), which has no `await` point — they have to stay
   synchronous.

2. **Deep + `Send` → `async` export, `self.spawn_on_runtime(fut).await`.**
   The AppHandle cloud methods. `spawn_on_runtime` (on `AppHandle`)
   spawns the future onto the bae runtime, so it runs on a 16 MB worker; uniffi
   only polls the shallow `JoinHandle` on the caller thread. The deep descent
   never touches the 0.5 MB stack.

3. **Deep + not `Send` → sync export, `on_deep_stack(|| async move { fut })`.**
   The onboarding `restore_*` functions build the new library's
   SQLite DB while downloading, so their future holds a `*mut sqlite3` across the
   await and is `!Send`. Both `tokio::spawn` and uniffi `async` exports *require*
   `Send`, so case 2 is impossible for them. `on_deep_stack` (a free helper,
   since these run before any `AppHandle`) builds the future and `block_on`s it
   on a dedicated 32 MB thread — the only way to give a `!Send` deep future a big
   stack. OAuth onboarding is `Send` and could use a worker, but shares this path
   so all onboarding is uniform.

## Cancellation

The uniffi fork (`bae-swift-cancel`) makes async exports cancellable: when the
Swift `Task` is cancelled, uniffi **drops the inflight Rust future**.

- `spawn_on_runtime` turns that drop into an `abort()` of the spawned task — a
  dropped `JoinHandle` otherwise just *detaches* and the work runs on to
  completion. coven's own drop guards (e.g. `AbortOnDrop`, connection release)
  then fire on the worker.
- The onboarding functions are synchronous (forced by `!Send`), so they aren't
  `Task`-cancellable; they tear down through their own `oauth_cancel` channel.

## Rule of thumb

A new bridge method that does **network / cloud IO** must run its work on a
worker — make it `async` and wrap the body in `spawn_on_runtime`; never
`block_on` a deep future on the caller thread. A method that only touches local
state stays synchronous and `block_on`s on the caller.
