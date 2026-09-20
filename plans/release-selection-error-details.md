# Release selection error details

## Queue and execution
Execute third, after `plans/remove-field-origins.md` and `plans/release-and-album-cross-reference.md` have landed. Use a separate focused branch in the same background worktree. Follow research, detailed implementation plan, regression tests, implementation, requirement-by-requirement review, normal verification/hooks, and coordinated fast-forward landing.

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
from `CoverArt`'s unexpected content/decoder detail. Reuse typed network,
timeout, and provider status cases; do not parse status codes from strings.
Client construction, invalid request construction, malformed image bytes,
unsupported decoder, and cache task panic remain diagnostic failures.
`HttpBodyError::Read` retains its transport/timeout classification;
`HttpBodyError::TooLarge` is a content failure with the actual limit retained.

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

## Queued successor
After this task lands, execute [Cover image formats](cover-image-formats.md) in the same worktree on its own branch. That contract adds Rust decoding for JPEG, PNG, GIF, and WebP consistently across remote, embedded, and local covers; it remains separate from generic unexpected-error diagnostics.
