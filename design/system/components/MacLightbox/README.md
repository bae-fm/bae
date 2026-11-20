# MacLightbox

The lightbox: the image on the backdrop, close, the arrows to the images before and after it, then at its foot the shown image's caption (`label`, with `source` under it, in white for the dark backdrop) over the thumbnails (`MacThumbnailStrip`: `images`, with `selected` the one shown). A lone image has no arrows and no strip, only its caption. `state`: `loading` or `failed`. `browse` adds Browse all images at the top.

**In the code:** macOS: `MacLightbox` (`Views/Components/MacLightbox.swift`).
