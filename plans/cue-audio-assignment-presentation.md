# CUE audio assignment presentation

## Contract

Execute at the position specified in [the serial plan](import-improvements-queue.md),
as a separate focused change. A CUE associated with several audio files must not
look unassigned. Show its assigned audio-file count, such as "8 audio files", in
the header. Keep the filename for a single-file association. Distinguish missing
and refused associations from resolved ones using authoritative core state.

Show a flat list of CUE FILE references and their current audio assignments:
requested filename → assigned filename. Each row offers that reference's existing
assignment control. Mark missing assignments visibly. Do not make users open a
nested submenu merely to inspect an association. Choosing or clearing one file
affects only that FILE reference, never the whole sheet or another reference.

## Evidence and design

ImportSheetBindingMenu labels its menu with containerName or "Choose audio…".
BridgeSheetBound.describesFiles has no single container, so a fully resolved
multi-file sheet incorrectly uses the unassigned label. Its menu nests each
reference's current association beneath the requested filename. Read the complete
projection, bridge, and macOS controls before editing. Reuse existing per-FILE
options and commands; derive presentation in the data layer. Do not invent a
second binding model or infer an assignment from filenames in the view.

Preserve the CUE filename's existing document preview, disc/ignored selection,
evidence chips, source audition, and editing permissions. Deleted included tracks
do not mean their CUE FILE associations have become unassigned. Keep association
status independent from whether a source currently contributes an included track.

## Assignment display does not depend on picker loading

The complete mapping projection and macOS caption controls expose a second
failure alongside the misleading placeholder. `SheetBound::DescribesFiles`
carries no container facts; `ImportSheetCaptionRow.hasBinding` finds neither a
container name nor a refusal reason for it. When editable options have not
loaded, or editing is unavailable, the entire association disappears. Loading
picker options changes that missing association into the incorrect Choose audio
label. Neither state reflects a change to the actual binding.

Carry the resolved count and the per-FILE current assignments in the
authoritative mapping projection. The caption and flat assignment rows must
remain readable before editable options load and when editing is disabled.
Options control which changes are offered; they must not be the only source of
the current association. Reuse existing association types and identities rather
than inferring bindings from the included track rows or adding persisted state.

The existing `SheetReferenceOptions` already names the requested FILE reference,
its assigned file ID, and allowed or refused choices. Its producer excludes an
audio file assigned to another reference. Preserve these reference-specific
constraints in the flat controls. A removed track does not clear that reference's
assignment, and an ignored sheet still has associations that can be inspected.

Test the caption and flat assignment list with editable options present, absent,
and pending, plus a read-only candidate. The resolved association must be the
same in each case; only interaction availability changes. Test partial bindings
with the actual missing reference visible, without presenting the resolved
references as unassigned.

## Projection and control implementation

Source inspection found that `CategorizedFiles::sheet_binding_options` uses
stored scan facts for codec and timing decisions; it does not probe files.
`ImportServiceHandle::sheet_binding_options` wraps that calculation in a
blocking task. Its comment claiming a probe per audio file is obsolete, as are
the matching comments in `ImportView` and `ImportSheetBindingMenu`.

Carry the existing `SheetReferenceOptions` values on each mapping sheet group,
through the canonical Rust bridge definition and its required callers. Reuse
that producer for current FILE assignments and permitted choices. The mapping
must include these values for ignored sheets and read-only candidates too.
Remove macOS's separate `sheetBindingOptions` state and asynchronous loading
path; the displayed mapping then owns both the association and its choices.
Keep any existing command/API that other platforms still consume, and correct
its obsolete probe documentation. Do not duplicate the option calculation.

The caption shows the assigned filename for one resolved file or the resolved
audio-file count for multiple files. Under it, render each FILE reference and
its current assignment in a flat row. Its menu contains that reference's
offered/refused audio choices and the existing clear action directly. Editing
permissions disable changes without hiding the current assignment. Keep the
sheet's overall unresolved/refused explanation and identify missing references
individually; do not call resolved references unassigned.

Update previews and tests to construct the canonical mapping shape. Existing
set-binding validation remains authoritative at command time, including stale
source checks and exclusion of audio assigned to another FILE reference.

## Canonical field contract

Add `reference_options: Vec<SheetReferenceOptions>` to `SheetGroup` and mirror it
as `BridgeSheetGroup.reference_options` (`referenceOptions` in Swift). Both
mapping construction sites call the existing categorized-files producer once per
sheet. Draft projection carries that same group through unchanged, so excluded
tracks cannot remove assignment facts or change the option list.

Change the resolved multi-file variant to
`SheetBound::DescribesFiles { audio_file_count: u32 }`, mirrored in the bridge.
The count is the resolved binding's physical audio-file count, independent of
included tracks. Single-file bindings retain their existing container facts.
Current per-reference assignments and choices remain the existing shared types;
bridge conversion carries them in both directions for the existing mapping
read-back API. No new persisted binding representation is introduced.

The core worker owns mapping production and regression tests; the parent owns
macOS controls, fixtures, and removal of the redundant loading path; the worker
owns canonical bridge conversion, required other-platform callers, catalogs,
verification, and the focused commit. Establish the hosted failure against the
unchanged UI before changing canonical fields, then regenerate all languages.

## Verification

Reproduce the resolved multi-file placeholder failure first. Test a single file,
multiple resolved files, partial/missing associations, codec/timing refusals,
ignored sheets, and removed included tracks with surviving source associations.
Exercise actual projection and macOS controls, including reference-specific
assign/clear commands. Preserve existing error handling and stale-result rules.
Use existing localized strings where suitable; translate additions in all relevant
locales. Run affected tests, app build, normal hooks, and requirement review before
committing and coordinating main integration. No live database or media edits.
