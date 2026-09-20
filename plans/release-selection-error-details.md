# Release selection error details

## Queue and execution
Follow the authoritative order in `plans/import-improvements-queue.md`: execute after release/album cross-reference enrichment has landed. Use a separate focused branch in the same background worktree. Follow research, detailed implementation plan, regression tests, implementation, requirement-by-requirement review, normal verification/hooks, and coordinated fast-forward landing.

## User contract
Only unexpected failures get diagnostic see/copy presentation. Expected failures, including handled HTTP 500/404 responses and other known provider/domain failures, remain in their established domain/status UI and are outside this task. Classify using existing error categories and caller semantics, never merely HTTP non-success or the presence of a selection error.

When selecting a release fails unexpectedly, the user must be able to see and copy the actual diagnostic details. Keep the friendly summary and Retry. Reuse existing app alert or inline error components; research `ErrorDetailDisclosure`, `ErrorAlert`, `DisplayError`, and existing copy behavior before choosing the surface. Do not invent a separate diagnostic presentation mechanism.

Do not discard the underlying cause across core, bridge, or Swift. OS logs and a generic localized description are not a substitute for visible details. Product UI scope is bae-macos; shared boundary changes must update canonical callers together.

## Evidence and research targets
`ImportSearchFlow+Identity.swift` catches metadata application failures, logs `error.localizedDescription`, and formats a generic display line. An artwork decoder failure can carry a useful cause explaining an unsupported GIF while the UI displays only a failed-import summary. Trace all release-selection error paths and preserve diagnostic payloads generically, not only for GIFs. Artwork codec support is a separate concern.

## Required behavior and tests
- Expected failures do not enter unexpected diagnostic presentation; unexpected failures do.
- An actual unexpected release-selection failure reaches an existing detail display component with the underlying diagnostic text.
- The user can copy that text using the app's established copy affordance.
- Friendly summary and Retry remain available.
- Retry, cancellation, stale responses, and selection changes retain their established behavior; details must belong to the failure currently shown.
- Reuse existing localized messages where possible. Any new or changed user-visible strings must be translated in every applicable catalog/locale.

## Verification
Add failing coverage for the actual failure-to-visible/copyable-details flow before implementation. Run relevant core/bridge boundary tests and macOS UI/state tests, build the app, review against this contract, run normal commit hooks, and report exactly what was checked.

## Source research

- The manual selection path is `ImportSearchFlow.applyMetadata` in
  `bae-macos/bae/bae/Views/Import/Search/ImportSearchFlow+Identity.swift`, through
  `Importer.applyCandidateExternalMetadata`, the bridge's
  `select_candidate_metadata_provenance`, and core's
  `ImportServiceHandle::set_candidate_metadata_provenance` in
  `bae-core/src/import/handle/scan.rs`. The latter prepares provider payloads,
  artist images, and cover bytes before committing the candidate revision.
- Provider errors remain typed in `ImportError::MusicBrainz` and
  `ImportError::Discogs`. `From<ImportError> for BridgeError` in
  `bae-bridge/src/types/configuration/settings.rs` currently retains their
  rendered detail but maps almost everything to `BridgeErrorCategory::Import`.
  That category cannot distinguish an expected HTTP 500 from an image decoder
  failure. `NotFound` is a separate bridge variant, but its entity kind is
  specifically a missing library; it must not represent a missing provider
  release.
- `ImportError::CoverArt { detail }` already loses classification earlier:
  `send_artwork_request` uses it for HTTP/transport failures, while
  `read_image_response` uses it for malformed or unsupported images. Cache task
  failure and HTTP client construction failure use it too. Optional artwork
  HTTP 404 returns `Ok(None)`; `fetch_required` turns that absence into an error.
  It also represents expected local-file conditions: `service/cover_image.rs`
  uses it when a selected cover disappeared or reading its file failed. A blanket
  conversion of every `CoverArt` into an unexpected diagnostic is incorrect.
  `cover_art_archive.rs::fetch_gallery` has a separate capped-body read that
  also flattens transport failures into `CoverArt`.
- The existing `LookupFailure` / `BridgeLookupFailure` pair already represents
  network, timeout, provider status, and diagnostic causes. The conversion in
  `import/search.rs` cannot be reused unchanged for manual selection: it maps
  provider `NotFound` to `Diagnostic` because its search callers intercept
  ordinary missing results themselves.
- Swift's `metadataApplicationError` converts the error to `String?` with
  `error.displayLine`; `ReleaseSelectionFailure` then stores only that string.
  This is where an already-preserved diagnostic disappears. `ImportStore`
  associates the failure with the current session and release identity; preserve
  those ownership checks.
- `ErrorDetailDisclosure` already displays the summary, bounded diagnostic
  excerpt, and the established `SystemActions.copyToPasteboard` action. Its copy
  control currently exists only when expanded text differs from the summary,
  so a one-line diagnostic of at most 180 characters has no copy button.

## Proposed implementation contract

### Classification before presentation

Keep `BridgeError`, `DisplayError`, and the existing error views. Do not add a
parallel release-selection error enum or a new diagnostic view. Existing
`Database`, `Internal`, `Config`, candidate refusal, and metadata mismatch
categories retain their meanings.

The existing `Import` category has both expected and unexpected producers, so
an additional distinction is required; checking for `.Diagnostic` is not
sufficient. Add `BridgeErrorCategory::ImportData` for an external/local source
that cannot be parsed or decoded, using the existing localized import-failure
key. Keep `Import` for established domain refusals. In the metadata-application
conversion, carry MusicBrainz `Other`, Discogs `Serialization`, `SourceData`,
and artwork decoding/content failures as `ImportData`; carry actual internal
and database faults using their existing categories. Do not relabel provider
HTTP responses as internal errors. Audit the entire match exhaustively rather
than using a catch-all that makes new domain errors unexpected automatically.

Preserve artwork request classification at its producer by composing the
existing `LookupFailure` in a request-failure variant of `ImportError`, separate
from unexpected content/decoder detail. Separate those content failures from
the expected missing/unreadable local-cover cases at their producers; do not
infer the distinction from the existing detail string. Reuse typed network,
timeout, and provider status cases; do not parse status codes from strings.
Client construction, invalid request construction, malformed image bytes,
unsupported decoder, and cache task panic remain diagnostic failures.
`HttpBodyError::Read` retains its transport/timeout classification;
`HttpBodyError::TooLarge` is a content failure with the actual limit retained.
Apply this distinction to both `read_image_response` and the independent
`cover_art_archive.rs::fetch_gallery` body read.

Expected manual-selection cases are explicit:

- MusicBrainz or Discogs HTTP 404 while loading a selected release: expected
  missing provider record, not an unexpected diagnostic.
- MusicBrainz, Discogs, or artwork HTTP 500 after existing retries: expected
  provider failure; retain the established summary/status treatment and Retry.
- HTTP 401/429, network loss, and timeout: expected provider/domain failures.
- Optional artwork HTTP 404: retain `Ok(None)` and existing cover choice policy;
  required artwork absence remains an expected request failure.
- Track-count/grouping mismatch, candidate already importing/imported, and
  other known editing/file conditions: existing domain presentation, no new
  diagnostic disclosure.
- Malformed provider JSON/data, unexpected decoder failure, a database fault,
  or broken invariant: friendly summary plus the complete diagnostic.

The precise expected provider status presentation must continue using the
existing lookup-status localizations wherever already present. This task does
not replace expected-error UI with a new view. Classification changes must not
alter request retry, provider-cache, draft-commit, or artwork fallback policy.
Manual release selection currently renders provider failures through the generic
`Import` category and contextual failed-load sentence; it has no typed status
presentation. Preserve that generic expected-failure line without diagnostic
detail. Search/identify surfaces already carrying `BridgeLookupFailure` retain
their status-specific presentation. Adding `ImportData` does not itself create
a status-bearing manual-selection category, and this task does not add one.

### Swift state and view

Change `ReleaseSelectionFailure.message: String` to a `DisplayError` value.
Change `metadataApplicationError` and the store's corresponding failure input
to retain that value. Preserve existing interpolated localized context strings
while retaining `DisplayError.detail`; do not build a newly translated prefix
when the whole sentence already exists in the catalog. For known expected
categories, retain the established line without diagnostic detail.

Render the value through `ErrorDetailDisclosure` inside the existing failed-row
slot, alongside Retry. Preserve matching by selected release, session identity,
audio identity, cancellation, replacement, and stale completion checks. File-tag
errors retain their existing pane surface; this task's new diagnostic
presentation applies to selecting an external release.

Make the shared component's copy action available whenever diagnostic text is
present, independent of whether it needs expansion. Keep the disclosure only
when expansion reveals additional text. Copy the complete original detail,
never the bounded summary or excerpt. Reuse the current icon, help text, and
`SystemActions.copyToPasteboard`; do not introduce another clipboard wrapper.

### Regression and verification sequence

1. Extend `ImportSearchFlowTests.swift` with an injected importer operation that
   throws an actual typed unexpected `BridgeError`, invoke production
   `applyMetadata`, and assert that the store retains the exact diagnostic for
   the selected release. Observe the missing-detail failure before editing the
   production flow.
2. Extend `ReleaseSelectionFailureTests.swift`'s hosted production release-group
   view test to use that failed flow. Assert friendly text and diagnostic text
   beneath only the failing pressing, activate the real copy control, and read
   the pasteboard to verify the unabridged diagnostic. Cover a short one-line
   diagnostic and a long/multiline one. Retain the existing Retry click test.
3. Add core/bridge conversion cases for expected provider 404 and 500,
   network/timeout, track-count/grouping refusals, unexpected source JSON,
   decoder error, and database/internal failures. Exercise real artwork HTTP
   classification with local responses so an HTTP 500 cannot accidentally pass
   as a decoder diagnostic. Preserve optional-cover 404 success behavior.
4. Retain and run current retry, cancellation, stale session, candidate reread,
   and selection replacement tests. Add a replacement-failure case with
   distinct diagnostics to prove no previous release's text remains visible.
5. Update canonical bridge category consumers and fixtures together, without
   changing unrelated platform presentation. Regenerate bindings through normal
   build scripts. Run relevant core/bridge tests, macOS state/view tests and app
   build, then normal hooks. Search for the removed string-only failure member,
   newly non-exhaustive category matches, and stale comments about log-only
   diagnostics in the changed paths.

This contract is research for the queued branch; no error-path implementation
is part of the release-enrichment change.

## Confirmed implementation and test files

The required macOS production changes are confined to the existing flow and
its retained failure: `Views/Import/Search/ImportSearchFlow+Identity.swift`,
`Services/ImportStore.swift`, `Services/Store/ReleaseSelectionFailure.swift`,
`Views/Import/Search/ImportSearchResultRow.swift`, and
`Views/Components/ErrorDetailDisclosure.swift` under `bae-macos/bae/bae/`.
`ImportStore.metadataApplicationFailed` must continue sending only `error.line`
to the existing string-valued file-tags pane error while retaining the complete
`DisplayError` for external-release failures. `ReleaseGroupListView` and its
intermediate views already carry `ReleaseSelectionFailure` unchanged.

`BaeKit/Sources/BaeKit/Services/DisplayError.swift` already retains the original
detail and supplies the 180-character first-line summary and 400-character
excerpt. It needs no replacement or duplicate type. Its `addingContext` helper
joins a prefix, so the flow should instead construct a `DisplayError` from the
existing whole interpolated localized sentence plus the original detail.
`SystemActions.swift` and `ErrorAlert.swift` already provide the clipboard
operation and another working consumer; no new clipboard service is needed.

Core producer changes belong in `bae-core/src/import/error.rs`,
`cover_art.rs`, and `cover_art_archive.rs`. Audit all existing `CoverArt`
constructors, including `service/cover_image.rs`, `service/importing.rs`, and
`service/mod.rs`, before deciding which producers require a changed variant.
Retain expected local-file cases rather than changing their classification to
make a match exhaustive. Bridge conversion, category declaration, and key
mapping belong in `bae-bridge/src/types/configuration/settings.rs`.
Update `bae-core/src/import/search.rs::import_error_to_lookup_failure` to pass
through the composed artwork request failure: its existing catch-all would
otherwise turn the newly typed expected failure back into `Diagnostic` on an
identify surface. Preserve the existing provider `NotFound` caller semantics
instead of sharing the manual-selection classification indiscriminately.
`ImportData` can be bridge-only like the existing metadata mismatch categories;
do not add it to `UiErrorCategory` without an actual core event producer.

The regression files are:

- `bae-macos/bae/baeTests/ImportSearchFlowTests.swift`: use its injected
  `Importer(applyCandidateExternalMetadata:)` closure and bounded completion
  wait to exercise production `applyMetadata` with typed errors. Add distinct
  replacement diagnostics and explicit cancellation coverage.
- `bae-macos/bae/baeTests/ReleaseSelectionFailureTests.swift`: preserve the
  existing hosted `ReleaseGroupListView` and Retry interaction; feed it the
  failure produced by the flow, rather than constructing a second failure by
  hand. Cover short and multiline diagnostics plus an expected failure with no
  diagnostic controls.
- `bae-macos/bae/baeTests/ImportStorePickTests.swift`: update the string-valued
  fixture and retain deselection, reread, audio-change, and removal coverage.
- `bae-macos/bae/baeTests/DisplayErrorTests.swift`: run the existing exact-detail,
  excerpt, first-line, context, and cancellation checks; add assertions only for
  changed behavior rather than repeating them in another model test.
- `bae-core/src/import/cover_art_tests.rs`: reuse the local HTTP response and
  request-count helpers through `RemoteImageCache::for_test()`. Assert actual
  404, permanent status, exhausted transient status, malformed content, and
  capped-body classifications, retaining cache and retry assertions.
- `bae-core/src/import/cover_art_archive.rs` tests: extend the existing actual
  gallery-fetch coverage to distinguish body transport failure from malformed
  gallery data and retain ordinary 404 empty-gallery behavior.
- `bae-bridge/src/types/configuration/settings_tests.rs`, a focused sibling
  module of `settings.rs`: exercise `BridgeError::from`
  for provider/domain versus data/internal/database causes. Register it in the
  owning module and include the existing localization-key coverage gate.

No current hosted test activates `ErrorDetailDisclosure`'s copy button.
`SnapshotTestSupport.hostInWindow`, `settle`, `capturePNG`, and `recognizedText`
already host and inspect the production view. The existing Retry test uses the
recognized text position and a real native control click or window mouse
events. That OCR path cannot identify the icon-only copy button. Give that
production button an accessible label using the existing localized “Copy
details” string and a stable accessibility identifier; locate the hosted
accessibility element and invoke its real press action. Verify the returned
action succeeds and `NSPasteboard.general.string(forType: .string)` equals the
entire original detail, including text beyond the excerpt. Do not call
`SystemActions.copyToPasteboard` directly from the test. Serialize the clipboard
interaction tests because they use the process-wide pasteboard, and restore
its previous contents after each test. Validate this hosted accessibility path
in the failing regression before relying on it; native-control discovery is
not yet demonstrated for this SwiftUI icon button.

Reuse `Localizable.xcstrings`' existing “Copy details”, “Details”, and contextual
failure sentences. If changing any wording, update every translation; otherwise
the category reuses the existing core import-failure key without catalog churn.
The current cross-platform source search found no exhaustive application-side
`BridgeErrorCategory` switch needing a new arm; regenerate canonical bindings
and let platform verification establish that rather than adding unused cases.

## Queued successor
After this task lands, execute [Cover image formats](cover-image-formats.md) in the same worktree on its own branch. That contract adds Rust decoding for JPEG, PNG, GIF, and WebP consistently across remote, embedded, and local covers; it remains separate from generic unexpected-error diagnostics.
