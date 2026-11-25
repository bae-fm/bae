# AndroidLightbox

A release's images full screen on the dark backdrop: Close at the top end, the image (`title` names the release whose art it shows) fitted to the width, and the caption (`AndroidGalleryCaption`: `label`, `page`, `count`) at the foot. `state` puts something else in the image's place: `loading`, a spinner over Loading the image…; `failed`, the centred notice Could not load image with Retry, on the backdrop in its own colours. `details` adds Details to the failed notice, for core's error behind it, such as a zoom whose tiles core could not read. `final` drops Retry, for a failure that trying again does not mend: a picture file the phone cannot decode, or a bug in the viewer.

**In the code:** Android: `AndroidLightbox` (`ui/albumdetail/AndroidLightbox.kt`).
