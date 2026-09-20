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
