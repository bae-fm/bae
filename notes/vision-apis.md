# Vision APIs in bae

Three places in bae talk to Apple's Vision / VisionKit frameworks. They look similar on the surface but use different APIs, run on different threads, and have different deadlock profiles. This doc spells out each one and the rules for calling them.

## Background: the cooperative-pool deadlock

Swift's cooperative concurrency pool is fixed-size — one worker per core per QoS, no overcommit. Every `Task { }` and `Task.detached { }` in the process shares those workers.

`VNImageRequestHandler.perform(_:)` is synchronous. Its signature:

```swift
func perform(_ requests: [VNRequest]) throws
```

Under the hood, `perform` fans work out across threads and sits in a `DispatchGroup.wait()` until they check back in. Both Dispatch and Swift concurrency pull from the same cooperative pool. When N concurrent `Task`s all call `perform`, N workers get pinned in the group-wait — but servicing the group needs a worker from the same pool. As soon as `N >= pool_width`, no worker is free, and `perform` can never return. The pool is deadlocked, not just slow. Every other `Task { }` in the app — thumbnail loads, Live Text, search, you name it — stalls too because the pool never drains.

Documented here:
- Swift Forums, ["Cooperative pool deadlock when calling into an opaque subsystem"](https://forums.swift.org/t/cooperative-pool-deadlock-when-calling-into-an-opaque-subsystem/70685) — reproducer is literally `VNImageRequestHandler.perform`.
- Saagar Jha, ["Swift Concurrency Waits for No One"](https://saagarjha.com/blog/2023/12/22/swift-concurrency-waits-for-no-one/).
- WWDC21, "Swift concurrency: Behind the scenes."

VisionKit's `ImageAnalyzer.analyze(_:orientation:configuration:)` is natively `async throws`. It suspends properly, frees its worker at each `await` inside the framework, and doesn't have this problem.

**Rule of thumb:** sync `perform` on Swift's cooperative pool is a landmine. Either use the async API, or hop to `DispatchQueue.global()` (which auto-scales — overcommits), or run it outside Swift's pool entirely (e.g. from Rust via `tokio::task::spawn_blocking`).

## 1. Live Text in the lightbox

**File:** `bae-macos/bae/bae/Views/Lightbox.swift`
**Framework:** VisionKit (high level)
**API:** `ImageAnalyzer.analyze(_:orientation:configuration:)` — natively `async throws`

```swift
private static let analyzer = ImageAnalyzer()
...
let config = ImageAnalyzer.Configuration([.text])
let analysis = try await Self.analyzer.analyze(nsImage, orientation: .up, configuration: config)
imageAnalysis = analysis   // fed to LiveTextOverlay (NSViewRepresentable over ImageAnalysisOverlayView)
```

- Properly async — safe to call from Swift's cooperative pool (`Task { }`).
- One call per viewed image. Result mounts as a transparent `ImageAnalysisOverlayView` on top of the image, which handles text selection/copy.
- No deadlock risk from this API itself. (It did used to hang when unrelated OCR work saturated the pool — see #2 — but that was cured by fixing #2.)

## 2. Background OCR for autocomplete

**File:** `bae-macos/bae/bae/Services/VisionArtworkAnalyzer.swift` (the `recognizeText(path:)` method)
**Framework:** Vision (lower level — `VN*` classes)
**API:** `VNRecognizeTextRequest` + `VNImageRequestHandler.perform([request])` — synchronous, blocking

```swift
func recognizeText(path: String) -> [String] {
    guard let cgImage = loadCGImage(path: path) else { return [] }
    var result: [String] = []
    let request = VNRecognizeTextRequest { req, _ in
        result = extractLines(req)
    }
    request.recognitionLevel = .accurate
    request.automaticallyDetectsLanguage = true
    let handler = VNImageRequestHandler(cgImage: cgImage, orientation: .up, options: [:])
    try? handler.perform([request])   // blocks THIS thread
    return result
}
```

- Sync on both sides — the completion handler mutates a local and `perform` returns synchronously.
- Exposed to Rust via the `ArtworkAnalyzerCallback` uniffi trait (same shape as the barcode path in #3). Rust calls it from `tokio::task::spawn_blocking`, so the thread owner is tokio's blocking pool, not Swift's cooperative pool. No cooperative-pool deadlock risk; no GCD hop needed.
- Artwork OCR is one of several text sources feeding the import search autocomplete. The full pipeline lives in bae-core's `CandidateTextService`, which harvests text from the candidate's folder path, folder-name brackets, audio/image/document filenames, CUE sheets, `.txt` content, and artwork OCR. Non-OCR sources emit a "fast pass" snapshot immediately; OCR streams on top per image. The classifier in `candidate_text.rs` filters noise, clusters OCR variants, and ranks by source weight, emitting `ImportEvent::CandidateTextScanUpdated` for the Swift reducer.
- Sequential (not parallel) by choice — the Neural Engine processes one ML request at a time, so parallelism buys no throughput and just multiplies the blocking-pool footprint.
- No Swift-side cache and no OCR state on the Swift side. The app store's `candidate.textScan` IS the cache for the session; across restarts, candidates don't persist and neither does OCR.
- Extracted text (catalog numbers, artist / album names printed on artwork, embedded in folder names, listed in CUE sheets, etc.) feeds the autocomplete dropdowns in `ImportSearchPane` when you're manually identifying a release.

## 3. Barcode scanning for auto-identify

**File:** `bae-macos/bae/bae/Services/VisionArtworkAnalyzer.swift`
**Framework:** Vision (same as #2)
**API:** `VNDetectBarcodesRequest` + `VNImageRequestHandler.perform([request])` — synchronous, blocking

```swift
let request = VNDetectBarcodesRequest { req, _ in
    payloads = (req.results as? [VNBarcodeObservation] ?? []).compactMap(\.payloadStringValue)
}
request.symbologies = [.ean8, .ean13, .upce]
let handler = VNImageRequestHandler(cgImage: cgImage, orientation: .up, options: [:])
try handler.perform([request])
```

- Same blocking sync `perform` as #2, but exposed to Rust via the `ArtworkAnalyzerCallback` uniffi trait. No continuation — the completion handler mutates a local and `perform` returns synchronously.
- Rust calls it from `tokio::task::spawn_blocking`, which uses tokio's blocking thread pool. That pool is independent of Swift's cooperative pool, so there's no cooperative-pool deadlock risk here — blocking from Rust's side is isolated.
- Extracts EAN-13 / UPC-A codes from cover art. Those codes drive the auto-identify flow via MusicBrainz / Discogs barcode queries.

## Summary

| Use case | Framework | Call shape | Who runs it | Deadlock risk |
|---|---|---|---|---|
| Lightbox Live Text | VisionKit | `await analyze(...)` | Swift `Task { }` | None — truly async |
| Autocomplete OCR | Vision | sync `perform` | Rust `tokio::task::spawn_blocking` | None — not on Swift's pool |
| Barcode auto-identify | Vision | sync `perform` | Rust `tokio::task::spawn_blocking` | None — not on Swift's pool |

## Rules for adding new Vision work

1. **Prefer VisionKit's async APIs** where they exist. `ImageAnalyzer.analyze` is async and safe to call from Swift's cooperative pool via `Task { }` — use it directly.
2. **Sync `VNImageRequestHandler.perform` belongs in Rust.** Expose the operation as a method on `ArtworkAnalyzerCallback` (the uniffi trait). Swift implements it as a plain synchronous function that calls `handler.perform([request])` directly — no `withCheckedContinuation`, no `DispatchQueue.global` hop. Rust drives the call from `tokio::task::spawn_blocking`, so the blocking happens on tokio's blocking pool, which is independent of Swift's cooperative pool. Do not call sync `perform` from a Swift `Task { }` or `Task.detached { }` — if you're tempted to, the operation belongs in core, not in Swift.
3. **Cap fan-out in core.** Even with the Rust-side bounce, dozens of concurrent Vision passes compete for the ANE (which serializes) and waste blocking-pool threads. Have core orchestrate sequentially, or with a small bounded concurrency (2-3), mirroring `CandidateTextService`.
4. **Keep orchestration out of Swift.** Per-candidate scan lifecycles, cancellation tokens, and state transitions live in core services, not in view `.task(id:)` blocks. Swift's job is to implement the platform trait method and to trigger core via a handle call; everything else flows back through events → reducer → store.
