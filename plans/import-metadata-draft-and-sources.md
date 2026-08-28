# Import metadata draft and prefill sources

## Outcome

Every import candidate owns one editable metadata draft. The draft exists independently of how it was populated. A person may type into it directly, replace it from an online release, replace it from file tags, edit the result, clear it, and apply another source without changing the candidate's files or mapping decisions.

Manual entry is the absence of a prefill source, not a metadata mode. MusicBrainz, Discogs, and File tags are provenance attached to the applied draft. Browsing a source is temporary UI state and is never stored as the candidate's identity.

## Domain model

- `CandidateMetadataDraft` is the candidate's stored editable release and track metadata.
- `MetadataProvenance` is optional:
  - an external release holds its provider and release ID;
  - file tags records that the current draft was last populated from the candidate's tag snapshot;
  - no provenance means the draft was populated directly or has been cleared.
- The draft owns its selected cover. Candidate files, file roles, sheet/audio bindings, disc assignments, and other mapping decisions remain outside the draft.
- Applying a source atomically replaces the draft, provenance, and selected cover derived from that source. It does not replace mapping decisions.
- Clearing metadata atomically replaces the draft with its blank shape and removes provenance and the selected cover. It does not delete candidate files, source snapshots, lookup evidence, file roles, bindings, disc assignments, or other mapping decisions.
- Editing a populated draft does not change provenance. Provenance answers where the current draft began, not whether it still equals the source.
- A blank draft is a valid stored candidate state but is not importable until required release and track fields validate.

The source browser is presentation state:

- closed: render the draft;
- Find online: render online search and identification in the metadata slot;
- File tags: render the tag-derived preview in the metadata slot.

Only one of those surfaces occupies the metadata slot. Opening a browser never changes the stored draft. Back closes it with the draft unchanged. Applying a result replaces the stored draft and closes the browser only after the authoritative candidate projection contains the replacement.

## Settings and automatic work

Import settings expose:

- `Default metadata source`: `Find online`, `File tags`, or `None`;
- `Identify automatically`: an independent switch whose effect is limited to Find online.

The default is applied when a candidate is discovered:

- `None` stores a blank draft and schedules no metadata extraction or lookup;
- `File tags` reads the candidate's tags, stores the snapshot, and applies it to the draft;
- `Find online` opens the online workflow for the candidate. With automatic identification enabled, it schedules signal extraction, OCR, barcode and disc-ID work, searches the providers, and applies a single confident result. With automatic identification disabled, it schedules no identification work.

Changing the default affects candidates discovered afterward. It does not replace existing drafts. Explicitly opening Find online may start automatic identification for that candidate according to the switch; when the switch is off, the browser offers an `Identify automatically` command beside the search form.

Automatic identification and explicit online search share the same source application command. The way a result was found is not persisted.

## Main pane

The candidate path header and the images, tracks, files, and mapping sections remain in place. The top metadata slot has these states.

### Blank draft

The release and track fields are editable immediately. Empty controls use labels and placeholders rather than an `Untitled` card or a `Manual` badge. The blank surface exposes `Find online…` and `File tags…` directly. It has no duplicate source actions in an overflow menu.

### Populated draft without provenance

The same editable release card shows its cover, release fields, storage choices, and import action. It shows no source line or badge. Its overflow menu contains `Find another release…`, `Use file tags…`, and `Clear metadata`.

### Populated draft from MusicBrainz or Discogs

The editable card shows a MusicBrainz or Discogs chip in the existing source-chip position. The chip opens the exact applied release on that provider. Later edits leave the chip in place. The overflow menu contains the same three replacement and clear actions.

### Populated draft from file tags

The editable card shows a quiet `File tags` chip. The overflow menu contains the same three replacement and clear actions.

The cover square is a drop target in every draft-card state. Dragging a candidate image onto it selects that cover, shows a hover target while dragging, and leaves provenance unchanged. Clearing metadata clears the selection but leaves the image in the candidate gallery.

### Find online browser

The browser replaces the draft card and starts with a Back button, provider control, and search form. It shows identification progress and signal chips when identification runs, followed by results in the same section. When automatic identification is disabled it also shows `Identify automatically`.

Selecting a result keeps the browser open and shows progress in that row while full release data is fetched and stored. The browser closes only after the candidate projection confirms that exact release as the applied provenance. Errors and empty results remain in the browser with search and retry available.

The populated card's `Find another release…` action opens this browser again without changing the draft.

### File tags browser

The browser replaces the draft card and starts with a Back button. It reads the candidate's current tag snapshot and shows loading, then one tag-derived preview with an Apply action. Apply remains in progress until the candidate projection confirms File tags as the applied provenance, then returns to the draft card. Errors remain in the browser with retry available.

## Candidate list

The list does not describe an idle blank draft as `Needs metadata`, `Waiting to be identified`, or another request for attention. It shows activity only while work is actually queued or running, and shows stored identification/import outcomes that require a decision.

A candidate with a blank or manually populated draft remains an ordinary Pending row. Readiness is derived from whether its current draft and mapping validate for import, not from whether it has provenance.

## Persistence and commands

The device-local candidate tables store the draft separately from optional provenance. The schema does not encode a Manual source. Candidate creation writes the configured initial draft state in the same transaction as its candidate state. Source application and clear operations each use one database transaction.

The core owns these operations and the bridge exposes them without reproducing their rules:

- open or explicitly start online identification for one candidate;
- apply an external release to the draft;
- prepare and apply file tags to the draft;
- save draft edits and the selected cover;
- clear the candidate metadata draft;
- set the default metadata source;
- set automatic online identification.

The candidate detail projection carries the draft, optional provenance, validation/readiness, and current background activity. Platform UIs render that projection and hold only the temporary open-browser state.

The library release model also permits no metadata provenance. Releases imported from direct entry store no provider identity and no source release ID. Existing-artist selection remains the same assignment mechanism for every draft: search library artists by ID or name, retain an existing artist ID when selected, or retain a new-artist seed otherwise.

## Implementation

- Replace import metadata mode configuration with default metadata source plus the online automatic-identification switch. Remove last-used mode.
- Separate candidate draft persistence from provenance and remove the Manual seed/source variants throughout the core, bridge, generated-language consumers, tests, previews, and documentation.
- Initialize a blank draft at candidate discovery and dispatch the configured default source exactly once for that candidate.
- Make source application and clearing transactional and preserve mapping state by construction.
- Update queue projection so idle source-less drafts are ordinary Pending rows and actual queued/running identification retains activity status.
- Replace the macOS segmented metadata modes with the single draft/source-browser slot, including result-row completion, file-tag apply completion, source provenance chips, overflow actions, confirmation on clear, and image drop.
- Update Avalonia to the same bridge model and settings so every generated binding consumer compiles against the canonical types.
- Translate every added or changed user-visible string in every locale catalog.
- Add positive tests for candidate initialization under each default, explicit source application, source-less draft editing/import, transactional clear preservation, source provenance projection, queue placement, automatic online scheduling, and source-browser completion.

## Verification

- Search the repository for the removed mode, Manual seed/source, last-used setting, and Needs-metadata list state.
- Run the dependency-aware commit hook.
- Run focused core import/database tests and macOS tests for the draft, settings, list, source browsers, and cover drop.
- Build the macOS, Android, and Avalonia binding consumers affected by the bridge change.
- Commit on `main` and push after local verification passes.
