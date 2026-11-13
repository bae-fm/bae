# Mocks

bae's screens as they should work, drawn on a claude.ai Design canvas. Each
`.dc.html` file is one board, named for its board: `<platform>.<area>.<screen>[.<state>]`
(`mac.settings.subsonic.port-in-use`), as
agent-instructions' `principles/design-to-implementation.md` sets out;
`canvas.json` places the boards on pages and holds the notes beside them.
A board another board mounts (`dc-import`) is a part, named
`<platform>.parts.<thing>`.

`shots.txt` lists, one name a line, the boards whose screens get a shot: the
app's real view drawn in a test and checked against the board. Explorations,
proposals, parts and boards that gather every state of a part side by side
have none; a board for a screen the code doesn't build yet joins the list
when it does.

`runtime.toml` records the canvas runtime the boards were published with (the
sha256 of the canvas's `artifact-type/dc-runtime.js`), so the shot check
draws boards with the same one.

Yellow notes beside a row name what the code needs.

The canvas is published at https://claude.ai/artifact/9geFwGqEzevbvbh22NqZzJ.
These files are its source and the published canvas is a copy: an edit made
on the published page is not kept, and the next publish overwrites it with
these files.

Boards mount the design system in `../system/` from a copy under `ds/bae/`,
which publishing builds from `../system/` and git doesn't hold.
