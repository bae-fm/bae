# MacCoverGallery

The cover gallery shows a Source pop-up button, tiles under Current cover, Remote sources and Release files, a preview with ‹ N of M ›, and a Use this cover button. A tile shows its image and its name and no source line, since its section's heading says where it comes from; the gallery names the current cover's tile Cover, as a release's gallery names its cover, and core numbers each catalog image (Discogs · 1 · Front). The preview names the selected image and its source, which no heading says there. `remote: 'unlinked'` shows No linked release and drops Refresh, which has nothing to fetch.

Each tile is a `MacCoverTile`, and the preview is a `MacCoverPreview`.

**In the code:** macOS: `MacCoverGallery` (`Views/Import/Confirmation/MacCoverGallery.swift`) in `CoverSheetView`.
