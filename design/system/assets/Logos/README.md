# Logos

`bae-sheep.svg` is the sheep from `icon-source.svg` at the repository's root, the one copy to edit. It has no background. On an app icon it sits on a pale pink tile, `#FFD6E7`.

`bae-favicon-16.svg` and `bae-favicon-32.svg` are the sheep drawn pixel by pixel for favicons and icons at those sizes. They are their own drawings, not scaled copies, so a change to the sheep is redrawn here by hand.

`bae-word.svg` is "bae" in Fredoka SemiBold, turned into a drawing. It's white here; `Logo` uses it as a mask, so it takes the text colour around it.

The Mac app's asset catalog (`bae-macos/bae/bae/Assets.xcassets`) symlinks `icon-source.svg` and `bae-word.svg`, so each has one copy to edit.
