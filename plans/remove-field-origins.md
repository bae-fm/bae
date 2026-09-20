# Remove field-origin tracking

## Contract
Applying metadata replaces the supplied editable metadata, including values a person previously typed. Remove the per-field origin and disagreement model rather than retaining empty fields or compatibility adapters. Preserve metadata source identities, selected release records, archived payloads, evidence-file origins, current editable values, audio bindings, and explicit reset/apply operations.

## Research
`FieldOrigin`, `FieldOrigins`, `FieldValues`, `FieldClaims`, `FieldClaim`, `FieldDot`, and `FieldProvenance` feed draft persistence, library persistence, bridge forms, automation output, and Avalonia dots. `LibraryManager::carry_typed_release_fields` protects typed fields during File Tags re-identification. `CandidateEditField` independently identifies edit commands and must remain.

## Implementation
1. Change the existing File Tags preservation test to require replacement, including clearing an absent tagged field; run it against the old implementation and record the expected failure.
2. Retain `CandidateEditField` in a matching module. Delete field-origin types, derivation, claims extraction, typed-field preservation, and source-origin stamping across core draft, imported release, and payload paths.
3. Add an ordered database migration removing the eight origin columns on candidate edits and imported releases. Preserve every other column and row. Update SQL reads/writes and historical-migration tests without modifying historical migrations. Test populated upgrade preservation.
4. Remove bridge field-origin types/functions/projections, raw/wire edit origins, and reset/seed provenance arrays. Remove automation-only mirrors and update every caller and fixture.
5. Remove Avalonia origin/disagreement dots and their interaction plumbing; remove macOS vestigial form-origin state. Keep field editing, reset, save, cancel, and metadata source selection behavior. Remove obsolete localized strings across catalogs.
6. Replace origin-specific tests with behavioral coverage where useful; delete tests whose sole subject is the removed mechanism. Search code, tests, notes, and generated-binding consumers for stale APIs.

## Verification and review
Run the failing replacement regression again; run core library/import/database/migration tests, bridge and automation tests, platform compile checks and relevant metadata form tests. Run normal dependency-aware commit hooks. Review the diff against each contract requirement, verify no old origin API remains except immutable historical migrations and migration upgrade fixtures, and inspect migration data preservation. Existing unrelated CI failures must be reported separately, never hidden by claims of full success.

## Delivery
Commit on the isolated background worktree branch after checks. Coordinate the fast-forward merge into current main with the parent agent; rebase if needed. Push after merge. Do not write the live database or source media.

## Queued successor
After this removal lands, execute [Release and album cross-reference enrichment](release-and-album-cross-reference.md) on a separate branch in this same worktree. The successor's complete agreed contract is persisted there so execution survives conversation compaction.

## Verification prerequisite before enrichment
Avalonia currently fails compilation on stale per-FILE CUE binding callers in `NativeBae.cs`: binding options are grouped by FILE reference, and `SetSheetBinding` requires that reference. After the field-origin removal commit, execute [Avalonia CUE FILE reference bindings](avalonia-cue-file-reference-bindings.md) in a separate focused commit/branch before enrichment and run the affected view tests. Trace the existing per-FILE model; never invent a reference or silently use the first one. This prerequisite is authorized to verify all canonical bridge callers, not to add unrelated Avalonia features.

## Verification receipt
The replacement regression failed against the previous implementation (`Typed Label` survived instead of becoming absent) and passes after removal. The core library run passed 2,193 tests and exposed an invalid identification-method value in the new migration fixture; correcting that fixture to `catalog_number` made its focused upgrade test pass. Core integration suites passed 54 tests, automation passed 29, and desktop bridge passed 60. Regenerated native bindings built the macOS app; 20 focused editor/import-store tests passed, followed by an eight-test editor rerun after using the existing bounded asynchronous wait helper. Avalonia verification reached the known per-FILE caller compilation errors; its separately queued repair owns that verification prerequisite.

Requirement review confirmed removal of per-field origin types, storage, claims calculations, UI dots, and typed-value protection while retaining field commands, actual values, metadata records and archives, evidence origins, and audio bindings. The populated migration comparison preserves all retained values across thirteen tables. Searches found old field-origin APIs only in historical migrations, migration-upgrade fixtures, and plans. Review removed four stale origin-stamping comments. Normal commit hooks remain the final local gate.
