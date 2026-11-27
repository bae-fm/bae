# Logos

`bae-sheep.svg` is the sheep from `icon-source.svg` at the repository's root, the one copy to edit. It has no background. On an app icon it sits on a pale pink tile, `#FFD6E7`.

`bae-favicon-16.svg` and `bae-favicon-32.svg` are the sheep drawn pixel by pixel for favicons and icons at those sizes. They are their own drawings, not scaled copies, so a change to the sheep is redrawn here by hand. The website's `public/` folder symlinks both, and every page of bae.fm uses them as its favicons.

`bae-word.svg` is "bae" in Fredoka SemiBold, turned into a drawing. It's white here; `Logo` uses it as a mask, so it takes the text colour around it.

`forms.json` holds how the logo's pair (the sheep over the word) and its inline form (the sheep before the word) lay the word out: the word's width and the gap between the sheep and the word, each a fraction of the sheep's width. The design system's `Logo` and the website's logo both read it, and each takes the word's and the sheep's proportions from their drawings' `viewBox`.

The Mac app's asset catalog (`bae-macos/bae/bae/Assets.xcassets`) symlinks `icon-source.svg` and `bae-word.svg`, so each has one copy to edit. The Android build converts both to vector drawables at build time.

`sheep-parts/` holds the sheep cut into the parts `ImportSheep` moves, one drawing each, all in the logo's own box so they stack in place. They are drawings in their own right: a change to the sheep is made to them too. The Mac app's asset catalog links each part.
