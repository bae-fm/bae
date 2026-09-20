# Avalonia CUE FILE reference bindings

## Contract and order

After the field-origin removal commit, repair Avalonia's callers of the existing per-FILE CUE binding API in a separate focused commit. Execute this prerequisite before [release and album cross-reference enrichment](release-and-album-cross-reference.md), using the same agent worktree. This repairs canonical bridge callers; it does not change the import draft model or broaden Avalonia's feature scope.

Each CUE FILE reference has its own current association and available audio choices. Present every reference core returns, and send the selected reference unchanged when assigning or clearing it. Never infer a FILE reference from the sheet filename, the audio filename, or the first reference. Preserve core's refusal decisions and existing error presentation. Do not edit the live database or source media.

## Observed failure and current data

The affected Avalonia view-test build fails in `NativeBae.cs`: it treats `BridgeSheetReferenceOptions` as a single `BridgeSheetBindingOption`, accessing an absent `Offer`, and calls `AppHandle.SetSheetBinding` without the required `fileReference` argument. The same sheet-wide assumption exists through `ImportService`, `ImportStore`, `ImportMappingPane`, `ImportMappingActions`, and `ImportMappingTable`.

Core's `CandidateFiles.sheet_binding_options` returns an ordered list of `SheetReferenceOptions { file_reference, file_id, options }`. Each options list contains `{ file_id, offer }` entries. It excludes audio already assigned to a different FILE reference and validates timing/codec/readability per reference. `set_sheet_binding(candidate, sheet, file_reference, audio_file_id)` validates the named reference and selected audio. A null audio ID explicitly clears that reference. The current association must therefore come from the reference's `file_id`, not the sheet-wide `Bound.Container()` projection.

macOS already passes this shape through and renders a submenu for each FILE reference. Avalonia currently flattens options into `ImportSheetBindingOption` and builds one sheet-wide ComboBox. That flattening cannot represent a multi-file CUE and must be removed.

## Implementation

1. Read the matching full rules and the complete affected source/test files before editing. Add regression tests against the real mapping table and store. Record the baseline compilation failure before implementation; the current build cannot reach runtime assertions.
2. Return `List<BridgeSheetReferenceOptions>` directly from `NativeBae.SheetBindingOptions`, through `ImportService`, `ImportStore`, and the mapping table's options callback. Delete `ImportSheetBindingOption`; it loses the reference grouping and duplicates bridge state. Continue resolving refusal text through `BridgeDisplay.RefusalLine` when rendering.
3. Add the required `fileReference` argument to `NativeBae.SetSheetBinding`, the service delegate, store method, pane handler, and `ImportMappingActions.BindSheet`. Preserve candidate key, sheet ID, FILE reference, and optional audio ID at every hop. Update every constructor and fixture call site in the same commit.
4. Replace the sheet-wide ComboBox with a binding menu. Its entries are FILE-reference submenus in core's order; each contains that reference's offered/refused audio options and the existing localized clear-association action. Read current selection from that reference's `FileId`. Show refused choices disabled with their localized reason. Clearing sends null for only that reference. Populate without dispatching any mutation. A reference with zero available audio still exists and can display its absent/current association; an empty reference list presents no choices. Use the existing sheet caption and localized audio-choice wording; do not add speculative state or strings.
5. Keep options loading on the existing injected asynchronous path. Errors remain visible through `ImportStore`'s existing banner path; stale-handle results remain ignored. Preserve preview, sheet disc selection, role editing, and evidence display.
6. Correct comments on touched APIs that claim multi-file sheets return no choices or that binding applies to the entire sheet. Include the bridge method documentation if necessary; do not change the bridge signature or core behavior.

## Regression evidence

- Mapping table given two references renders both in order, with their own current selected audio and independently offered choices. Construct the sheet's summary without a single container to prove selection comes from reference data.
- Choosing audio for the second reference dispatches exactly `(sheetId, secondReference, chosenAudio)`; clearing the first dispatches `(sheetId, firstReference, null)`. Merely loading/rendering the options dispatches nothing.
- Refused codec/timing/unreadable entries remain visible and disabled with localized reasons. Core's offered entries remain enabled. A reference with no candidates still has its clear action; no references means no binding options.
- ImportStore passes the candidate, sheet, reference, and nullable audio through to its injected ImportService. Include success and error/stale-session behavior without invoking a real database.
- Run the existing mapping-table suite and the metadata form/marks/verification suites that could not compile during origin removal; then run the full affected Avalonia view-test project if those succeed.

## Verification and delivery

Reuse the generated C# bindings from the verified bridge library, or regenerate using `uniffi-bindgen-cs --library target-validation/libbae_bridge.dylib --crate bae_bridge --out-dir bae-bridge/csharp-bindings-full --no-format`. Do not create an alternate Cargo target directory. Keep generated artifacts out of the commit.

Run `dotnet test bae-avalonia/bae-avalonia.ViewTests/bae-avalonia.ViewTests.csproj --framework net8.0 -p:TargetFrameworks=net8.0` with the validated bridge library and FFmpeg directories on `DYLD_LIBRARY_PATH`, using targeted filters first. Run C# formatting, stale API searches, `git diff --check`, and normal dependency-aware commit hooks. Review every contract point and report platform/runtime checks not executed. Commit this concern separately after the removal commit; coordinate branch and fast-forward integration with the parent agent before starting enrichment.
