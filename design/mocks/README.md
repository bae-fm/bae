# Mocks

bae's screens as they should work, drawn on a claude.ai Design canvas. Each
`.dc.html` file is one board; `canvas.json` places the boards on pages and
holds the notes beside them. Blue monospace text inside a board is a note, not
part of the design; yellow notes beside a row name what the code needs.

The canvas is published at https://claude.ai/artifact/9geFwGqEzevbvbh22NqZzJ.
These files are its source: an edit made on the published page is copied back
here and committed before the canvas is published again.

Boards mount the design system in `../system/` from a copy under `ds/bae/`,
which publishing builds from `../system/` and git doesn't hold.
