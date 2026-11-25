# MacLightbox

The lightbox: the image on the backdrop, the Close button, the Back and Forward buttons to the images before and after it, then at its foot the shown image's caption (`label`, with `source` under it, in white for the dark backdrop) over the thumbnails (`MacThumbnailStrip`: `images`, `names` that name each thumbnail by its caption, with `selected` the one shown). A lone image has no arrows and no strip, only its caption. `state`: `loading`, the spinner named by Loading the image…, or `failed`, the centred notice Could not load the image with Retry; `details` adds Details for a failure with causes behind it, such as a zoom whose tiles core could not read. `browse` adds the Browse all images button at the top.

**In the code:** macOS: `MacLightbox` (`Views/Components/MacLightbox.swift`).
