# ImportSheep

The sheep on the Import screen while it has no folders, above "Add a folder to import music from". Every few seconds it plays one moment, then rests for two to three seconds. The moments come from a fixed sequence of eight that then starts again, 57 seconds in all. No moment follows itself, and each brings different releases: made-up covers, records, CDs and sleeves in the accent fills, outlined in the sheep's plum. Six of the eight bring releases in; two are play.

| Starts at | Moment | Lasts, with its rest |
| --- | --- | --- |
| 0 s | Gobble from the right | 7 s |
| 7 s | Catch from above left | 7.5 s |
| 14.5 s | Bop | 7 s |
| 21.5 s | Gobble from the left | 7 s |
| 28.5 s | Breathe in from the right | 7.5 s |
| 36 s | Catch from straight above | 7.5 s |
| 43.5 s | Roll | 6 s |
| 49.5 s | Catch from above right | 7.5 s |

- **Gobble:** a line of three releases slides in from one side on a steady beat. The sheep turns its face to them, opens wide and chomps each as it arrives, munches with full cheeks, swallows, and the heart on its player pulses.
- **Catch:** one release spins down from above, past the side it comes from, or straight down. The sheep startles, with marks of surprise on that side, looks toward it, crouches, leaps and catches it into its player, whose screen flashes. It lands with a squash in a burst of stars, blushes, and a heart floats up.
- **Breathe in:** the sheep turns to one side and opens its mouth round. Streaks of air rush toward it; a release far off wobbles, then is pulled in faster and faster and vanishes into the mouth. Cheeks and head swell full, it holds, then gulps.
- **Bop:** the heart on its player starts to pulse and it bops to the music in its headphones for eight beats: nods, sways, three little hops, ears flapping.
- **Roll:** it tucks its head, springs up, rolls once on the spot, lands with a squash and wobbles back upright.

The sheep is never mirrored, so the heart stays on its right earcup and the player stays where the logo has it. To face left or right it turns its face: the eyes, cheeks and mouth slide toward that side inside the face, the far cheek narrows, and the head leans.

`size` is the sheep's width; Import shows it at 140, with `space-section` between it and the words. The drawing's box is wider and taller than the sheep, to leave room for the releases on their way in; they fade in at its edges, and nothing is drawn outside it.

When the person has asked for reduced motion, it holds still: the sheep at rest with the sequence's first release, a blue cover, beside it. `still` shows that pose anywhere.

The sheep is the logo's own drawing, split into its moving parts, so it can't drift from the logo. The only new drawings are the mouth opened wide, the munching mouth, the blush lines, the marks of surprise, the stars, the streaks of air and the heart that floats up. Each part turns and squashes around its own pivot, listed in `bundle.css`. Each moment's timings are keys in seconds in `src/36-import-sheep.js`, and the sequence is the list near the end of that file; the moments join end to end into one loop of CSS keyframes.

**In the code:** macOS: `ImportSheep` (`Views/Import/Sheep/ImportSheep.swift`), drawn by `ImportSheepPainter` on a `Canvas` in a `TimelineView(.animation)`, paused and showing the still pose under `accessibilityReduceMotion`. The parts are the vector images in `assets/Logos/sheep-parts`, which `design/system/split-sheep.py` cuts from `icon-source.svg`; the moments' keys are `SheepMoment+*.swift`, a port of `src/36-import-sheep.js`, with `UnitCurve.bezier` for the easing curves.
