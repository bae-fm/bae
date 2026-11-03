# Synthetic cover images

These fixtures contain generated colors, with no external artwork.

| Files | Pixels |
| --- | --- |
| `solid.gif`, `solid.webp` | 16 × 8, RGB (120, 40, 200) |
| `animated.gif`, `animated.webp` | Two 1200 × 600 frames: RGB (120, 40, 200), then (20, 220, 40); 100 ms per frame |
| `transparent.gif` | 16 × 8: opaque RGB (120, 40, 200) in the left half; transparent right half |
| `transparent.webp` | 16 × 8, RGBA (120, 40, 200, 128) |

The solid GIF and WebP are 51 and 38 bytes respectively. They exercise valid
images below a 100-byte threshold. The animations exercise first-frame selection
and aspect-preserving downscaling; the static images exercise no upscaling.

Generate source PNGs with ImageMagick `magick -size WIDTHxHEIGHT xc:COLOR`.
Convert solid GIFs with `magick source.png solid.gif` and WebP with
`cwebp -lossless source.png -o solid.webp`. Encode animation using
`magick -delay 10 first.png second.png -loop 0 animated.gif` and
`img2webp -lossless -d 100 first.png second.png -o animated.webp`.
The transparent WebP uses `cwebp -lossless -exact` on an RGBA PNG. The transparent
GIF uses a transparent canvas with a rectangle covering x=0..7 and y=0..7.
