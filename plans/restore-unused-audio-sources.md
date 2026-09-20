# Restore unused audio sources

## Queue and execution
Execute after [cover image formats](cover-image-formats.md), on a separate focused branch in the same background worktree. The serial order is field-origin removal, Avalonia per-FILE CUE caller repair, release/album cross-reference enrichment, unexpected release-selection diagnostics, cover format support, then restoring unused audio sources. Follow research, detailed plan, regression tests, implementation, contract review, normal hooks, and coordinated fast-forward integration.

## User contract
Audio removed from the release's track list remains visible and individually addable. Use a plus button with the localized hover text “Add track”. Do not use “draft” in product UI. Adding one source must not reset any other track or reapply the selected release's metadata as a whole.

Cover whole audio files and individual slices from the currently selected CUE sheets. Prevent duplicate inclusion, preserve source order, respect current CUE selection and FILE bindings, and never restore a slice that is unavailable under the current source configuration. Keep the audio-backed track model and one-time metadata application; do not add field-origin or manual-edit protection machinery.

## Confirmed research starting point
The existing apply_track_edit path in preparations/pane_edits.rs removes CandidateTrack outright. mapping.rs draft_mapping_table gathers source units but renders the included draft.tracks and omits unused units. Consequently, both whole files and CUE slices can lose their visible route back into the release.

Read the complete production path before implementation, including source unit identity/order, track removal and candidate persistence, mapping projection, source selection changes, and existing track construction/defaults. Preserve removal as actual removal of the included row; represent the remaining available audio independently from included tracks rather than adding a soft-delete flag.

## Metadata and numbering design to settle before implementation
Document the exact metadata and number assignment rules after tracing existing source-to-track constructors. Reuse the same source metadata/default policy that creates audio-backed tracks, including the pre-fill-with-tags preference, explicit CUE metadata, and the required number default based on ordered track position. Specify whether and how re-adding can reuse a removed row's values without introducing hidden field-history or retention mechanisms; the contract does not authorize silently restoring stale release metadata.

The plan must explicitly cover a removed whole file, a removed selected CUE slice, mixed existing edits, absent source title/number, and source order across multiple discs. State how a new track's number is chosen without renumbering or rewriting existing rows. Do not infer an audio-to-release match merely from list position or reapply all selected catalog metadata to accomplish one addition.

## Required regression cases
- Remove a whole-file track; its source stays visible with Add track. Add it and verify the actual included row returns while all other metadata and audio assignments remain unchanged.
- Remove one slice from an active CUE; only that slice can be added, using its exact current file/sheet/index identity.
- Ignore or replace a CUE after removing a slice; unavailable slices are not offered or resurrected. Whole-file availability follows the current source configuration.
- Repeated or stale add commands cannot create duplicate inclusion. Source order remains deterministic after removing and restoring a middle track.
- Per-FILE CUE associations and partially unavailable sheets retain the existing source validity rules.
- Verify metadata/default/numbering decisions from the completed design against real production construction and persistence, not duplicated test logic.
- Exercise the actual macOS rendering and plus-button command path; translate the hover string and any other user-facing wording across all relevant catalogs.

## Separate observed issue
The macOS multi-file CUE header can say “Choose audio…” despite valid associations because ImportSheetBindingMenu uses containerName ?? placeholder and describesFiles has no single container name. This is an observed presentation issue pending design. Do not bundle a menu redesign into restoring unused audio sources.

## Delivery
Product work targets bae-macos; update shared canonical models and required callers together. Do not edit live database or media. Review the implementation against every contract item, run affected core/bridge/macOS checks and normal hooks, commit, then coordinate the main fast-forward landing.

## Queued successor
After individual source restoration lands, execute [Reset import setup](reset-import-setup.md). Reset is a separate menu action restoring the whole candidate to its initial scanned setup, including source tracks, automatic CUE/file choices, cover, and preference-dependent initial metadata.
