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
