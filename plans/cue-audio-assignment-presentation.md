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

## Verification

Reproduce the resolved multi-file placeholder failure first. Test a single file,
multiple resolved files, partial/missing associations, codec/timing refusals,
ignored sheets, and removed included tracks with surviving source associations.
Exercise actual projection and macOS controls, including reference-specific
assign/clear commands. Preserve existing error handling and stale-result rules.
Use existing localized strings where suitable; translate additions in all relevant
locales. Run affected tests, app build, normal hooks, and requirement review before
committing and coordinating main integration. No live database or media edits.
