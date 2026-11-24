# MacCoverGallery

The cover gallery shows a Source pop-up button, tiles under Current cover, Remote sources and Release files, a preview with ‹ N of M ›, and a Use this cover button. A tile shows its image and its name and no source line, since its section's heading says where it comes from; the gallery names the current cover's tile Cover, as a release's gallery names its cover, and core numbers each catalog image (Discogs · 1 · Front). The preview shows the selected image (`selected`), which is Cover unless given, since the sheet opens on the release's cover. It names the image and its source: Library for Cover, the catalog for a catalog's image, and Release files for a file. Its ‹ N of M › counts Cover, then the catalogs' images, then the files. `remote: 'unlinked'` shows No linked release and drops Refresh, which has nothing to fetch.

Each tile is a `MacCoverTile`, and the preview is a `MacCoverPreview`.

**In the code:** macOS: `MacCoverGallery` (`Views/Import/Confirmation/MacCoverGallery.swift`) in `CoverSheetView`.
