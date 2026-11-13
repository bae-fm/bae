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

A canvas holds at most 512 files, so the boards are published as two
canvases, split by page:

- https://claude.ai/artifact/XJtw4fspC9TpDxUPY8E2bJ: the welcome, settings,
  library, logo and shared pages.
- https://claude.ai/artifact/4BYSUYekneJvrGyTgaJ6AZ: the home, loading,
  search, import, storage and explore pages.

Each gets the boards on its pages and the part of `canvas.json` that places
them; no board mounts a part from the other canvas. These files are their
source and the published canvases are copies: an edit made on a published
page is not kept, and the next publish overwrites it with these files.

Boards mount the design system in `../system/` from a copy under `ds/bae/`,
which publishing builds from `../system/` and git doesn't hold.
