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

## Verification

Reproduce the resolved multi-file placeholder failure first. Test a single file,
multiple resolved files, partial/missing associations, codec/timing refusals,
ignored sheets, and removed included tracks with surviving source associations.
Exercise actual projection and macOS controls, including reference-specific
assign/clear commands. Preserve existing error handling and stale-result rules.
Use existing localized strings where suitable; translate additions in all relevant
locales. Run affected tests, app build, normal hooks, and requirement review before
committing and coordinating main integration. No live database or media edits.
