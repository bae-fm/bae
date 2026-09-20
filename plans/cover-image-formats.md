# Cover image formats

## Queue and execution
Execute after [release selection error details](release-selection-error-details.md), as a separate focused branch and commit in the same background worktree. The full serial order is field-origin removal, Avalonia per-FILE CUE caller repair, release/album cross-reference enrichment, unexpected release-selection diagnostics, then cover format support. Follow research, detailed implementation plan, regression tests, implementation, contract review, normal checks/hooks, and coordinated fast-forward landing.

## User contract
Rust decodes JPEG, PNG, GIF, and WebP cover inputs consistently for local folder covers, embedded artwork, and remote provider artwork. Enable the GIF and WebP features on the existing image dependency; do not change its version without evidence that it is required. Do not broaden support to every format macOS ImageIO can preview.

Keep the current normalized static JPEG output, with longest dimension at most 600 pixels and no upscaling. Preserve original folder files. For animated inputs, select the first image/frame, matching the existing macOS ImageLoader index-zero behavior. Handle transparency and color consistently with the existing JPEG cover contract; research that production path before choosing the expected compositing behavior.

## Evidence and boundaries
Remote provider cover responses can be HTTP 200 with image/gif, then fail the production RemoteImageCache validator because GIF decoding is disabled. A local folder.gif can already be automatically selected, then fail resize_cover for the same reason. Use synthetic committed fixtures, not live provider or personal-library data.

Audit selection, validation, and normalization together. Do not automatically offer other formats Rust cannot decode as covers. Keep the wider attachment-preview functionality: preview support does not imply eligibility for cover normalization. This task is independent of the generic unexpected-error diagnostic presentation task.

## Research and implementation requirements
1. Read the matching full rules and complete affected files. Trace local discovery and cover choice, embedded artwork extraction, remote validation/cache, cover resizing/storage, and macOS first-frame preview behavior.
2. Add regression tests against the actual production remote validator and resize/storage paths. Confirm GIF and WebP fail before enabling decoder support. Tests must not replace production validation with a decoder-only reconstruction.
3. Enable supported formats and align all cover eligibility checks with what core can decode. Preserve attachment previews and original source bytes.
4. Test synthetic static and animated input, first-frame selection, dimensions and no upscaling, color/transparency handling in JPEG output, malformed inputs, and existing decoding resource limits. Retain JPEG/PNG coverage.
5. Verify affected core/bridge/platform build configurations, run normal hooks, and review every contract requirement before committing. Report unexecuted cross-target checks accurately.

## Delivery
Use one focused change after the preceding queued tasks. Coordinate main fast-forward landing and push with the parent agent. Do not alter the live database or media files.

## Queued successor
After cover format support lands, execute [Restore unused audio sources](restore-unused-audio-sources.md) on its own branch. Unused whole files and selected CUE slices remain visible and can be added individually with a plus button and “Add track” hover text, without resetting other track metadata.

## Decoder research

The current remote validator in `import/cover_art.rs::read_image_response`
rejects bodies under 100 bytes before asking the decoder, then decodes without
`util/cover.rs`'s explicit resource limits. Valid compact GIF/WebP files can be
shorter than that heuristic; validation must use the actual decoder and the
same dimension/allocation policy as normalization. The existing byte-download
cap remains independent and required.

`util/cover.rs::resize_cover` currently writes the decoded `DynamicImage`
directly to JPEG. The locked `image` version is 0.25.10. Its
`DynamicImage::write_with_encoder_impl` calls the JPEG encoder's
`make_compatible_img`, which converts RGBA to RGB with `to_rgb8`; grayscale
alpha converts to grayscale. Alpha alone therefore does not establish an
encoding failure. Preserve this existing conversion contract and test it with
transparent PNG, GIF, and WebP input; do not add background compositing or a
second conversion solely on the assumption that JPEG rejects `DynamicImage`
alpha. Actual decoding remains subject to the shared resource limits.

`ContentTypeHint::is_raster_image` includes BMP because BMP remains previewable.
Do not redefine raster images to mean cover formats. Trace its consumers and
introduce a cover-specific eligibility predicate only where cover choices are
constructed; retain file classification and attachment previews.

## Confirmed cover-choice boundaries

- `import/local_artwork.rs::default_local_cover_file` and
  `import/service/cover_image.rs::pick_folder_cover` currently accept every
  raster extension, including BMP. Apply the four-format eligibility policy to
  automatic and explicit choices at both boundaries. Leave scanner image roles
  and attachment previews intact.
- `bae-bridge/src/types/conversion/mapping.rs` currently gives every artwork
  file a required cover choice. The macOS `BridgeCandidateFiles.images` list
  supplies both the cover picker and the artwork browser. Represent the absent
  cover choice for previewable unsupported files at the core/bridge boundary;
  select only eligible choices for the picker while keeping all artwork in the
  browser. Do not filter the shared images list and thereby hide attachments.
  Update required canonical callers and fixtures with the bridge shape.
- `import/file_tag_snapshot.rs::embedded_cover_from_tag` chooses a front picture
  or the first picture before checking its MIME type; its MIME conversion also
  accepts BMP. Make eligibility part of choosing the picture, so an unsupported
  front picture does not hide an eligible later picture. Keep front-picture
  preference among eligible pictures and existing file order. Both snapshot
  extraction and `read_embedded_cover` use this helper.
- `library/manager/image.rs::change_cover` normalizes both stored release files
  and remote choices with `resize_cover`. Verify this existing stored-cover
  path as well as the import funnel, and ensure its picker does not offer a
  format normalization rejects. Original release-file bytes stay unchanged.

`BaeKit/Sources/BaeKit/ImageLoader.swift` uses image index zero for both native
and thumbnail ImageIO decoding and preserves the image's alpha. It does not
flatten artwork onto a fixed background. Keep its wider preview behavior; Rust
cover normalization produces the existing static JPEG representation.

Factor the bounded byte decoder in `util/cover.rs` so remote validation and
normalization use the same detected-format allowlist, 8,192-pixel width/height
limits, and 128 MiB decode allocation limit. Remote validation must retain the
original accepted bytes and their detected content type, not cache normalized
JPEG bytes under the original type. Retain the independent HTTP body cap and
the request/content error classification established by the preceding task.
Remove the arbitrary 100-byte rejection only with a regression exercising the
real `RemoteImageCache` HTTP path. Do not build a second decoder in that test.

## Implementation ownership and projections

Use `ContentType::is_supported_cover()` as the JPEG/PNG/GIF/WebP policy.
`ContentTypeHint::is_supported_cover()` delegates through the existing image
content-type conversion; `path_is_supported_cover` applies that hint to source
paths. Keep image and raster classification unchanged. A shared bounded
`decode_cover` returns the decoded first image and its detected content type;
normalization uses the pixels, while remote validation preserves original bytes
and records the detected type.

Keep `ReleaseDetail.image_files` intact: the artwork browser, Avalonia, and
other consumers use it for previewable attachments. Project a distinct
`cover_files` collection in core for the persisted cover picker. Likewise,
project eligible candidate cover files in core and keep the complete candidate
artwork collection available. Candidate artwork's cover choice becomes optional;
core creates that choice only for an eligible source, and the bridge translates
it instead of building an unconditional choice. The picker consumes the core
projection without implementing MIME or extension rules in Swift.

Read `Candidate.swift::localCoverSelections` and its consumers before changing
that optional choice: it currently collects choices from every artwork file.
The image gallery must retain unsupported-but-previewable attachments when
those files stop offering a cover-selection action. Test a mixed BMP and
supported-image candidate and persisted release to establish both outcomes:
all images remain previewable, and only supported inputs are offered as covers.

Within this focused branch, the decoder worker owns the dependency features,
shared decoder/format policy, local/embedded selection, remote validation, and
those regression tests. The coordinating worker owns the core candidate and
release projections, bridge/generated-caller updates, macOS picker and preview
consumers, and stored-library cover tests. They share one worktree and one index
owner; native generation runs only after a stable Rust source handoff.

## Verification and review record

Production-path baselines rejected GIF/WebP in remote validation, folder import,
and stored-library cover replacement. Other regressions reproduced the compact
image byte heuristic, unsupported automatic BMP choices, retained embedded BMP
observations, and the macOS picker loading an unsupported image. These tests
exercise the real cache, scan/preparation/import, library manager, and hosted
picker rather than reproducing their algorithms.

The shared decoder accepts the four agreed formats and retains detected content
types for the original remote bytes. The first-frame, dimensions, no-upscaling,
transparency, malformed-input, dimension-limit, and allocation-limit tests pass.
Disabling the allocation cap reproduced the oversized GIF canvas failure before
restoring the original source. The limit constrains the image decoder's output
allocation; it is not a promise that dependency scratch allocations impose a
process-wide heap bound.

Stored file-tag snapshots remain observations. Automatic selection and import
skip unsupported embedded observations, while an explicit saved choice remains
visible and fails normalization rather than silently changing the user's choice.
The actual candidate import test proves both branches. The import-list preview
only reads embedded bytes for an explicit persisted selection, so it needs no
additional automatic-selection filter.

The requirement review traced each source through selection, validation, storage,
and display. Complete gallery collections remain unchanged; the distinct core
cover collections drive the pickers. Canonical Swift, C#, Kotlin, and preview
callers were updated together. No new localized string was introduced.

Local checks passed 125 cover tests, 34 file-tag mapper tests, seven import cover
tests, 15 snapshot tests, the retained-snapshot import and transparency cases,
69 desktop bridge tests, and all 262 Avalonia view tests. The Avalonia run first
failed because its generated native test library lacked the FFmpeg runtime path;
adding that path to the generated copy allowed the terminal successful rerun.
An initial macOS selection passed eight tests in the cover-picker and artwork
browser suites. Final native generation passed, followed by 14 macOS tests in
three suites: cover picker, artwork browser, and stored-release projection.
A native relink first failed with disk exhaustion; removing idle generated
compiler output allowed the successful retry. Android and iOS caller source
was updated, but no mobile cross-build was performed locally. CI is deferred to
the end of the queue under the user's execution contract.
