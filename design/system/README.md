bae is a music library app for macOS, Android, iOS, Windows and Linux. Its screens are quiet: dark surfaces in a chosen tone, one accent color, native controls, few words.

Everything here is drawn at each platform's regular size, the one the apps use by default. Other sizes, such as macOS's small and large controls, are variants to add later.

## Design terms

This README uses the design words that `builds help workflow` defines in DESIGN TERMS: board, shared board, canvas, design system, token, component, preview, design change, shot and app work. Read that section for what each word means.

## Guidelines

The design guidelines are review rules in agent-instructions, under
`projects/bae-rules/screen-content/`, so `builds review` checks every board and
screen against them. Each line names one rule file there.

- `build-each-screen-to-match-its-board.md`: every board draws the app as it
  should be, and the app changes to match it.
- `screen-words-say-only-what-the-ui-cannot.md`: sentence case, few words,
  periods, cut text, button verbs, placeholders.
- `storage-and-pinning-words.md`: the cloud, cloud home, pinning, moving to
  This Mac, waiting states.
- `notices-say-what-happened-and-the-fix.md`: what a notice says, Details,
  quiet notices, failed views.
- `colours-and-type-come-from-the-design-system.md`: tones, accents, colour
  names, status words, text styles.
- `one-primary-action-per-screen.md`: the primary button, colour in a row of
  buttons, Cancel.
- `each-platform-draws-with-its-own-parts.md`: per-platform components, settings
  layouts, spinners, empty lists.
- `dialogs-sheets-and-popovers-by-what-they-ask.md`: alerts, sheets, popovers
  and the three ways to fold things away.
- `text-in-a-block-shares-one-left-edge.md`: one left edge, markers hanging in
  the margin, leading columns.
- `menus-size-to-their-items.md`: a menu's width and its separator lines.
- `add-controls-say-add.md`: the words on a control that adds to a list.
- `page-headers-follow-the-album-page.md`: the header of a page about one
  thing.
- `board-sample-data-agrees-with-itself.md`: counts, totals and names that
  agree.
- `component-previews-hold-their-content-still.md`: a preview's named rows.
- `icons-and-status-glyphs.md`: Material Symbols, SF Symbols, the status ring.
- `the-sheep-logo-and-its-forms.md`: the sheep, the drawn word and where each
  form goes.
- `no-restyling-a-component-from-outside.md`, `no-music-note-imagery.md` and
  `no-real-artist-album-song-names-in-artifacts.md`.
- In `rules/screen-content/`: `no-decorative-icons-or-emoji.md` and
  `no-llm-slop-chips.md`.

## Shots

The code is checked against this system in two kinds of shot (agent-instructions `principles/design-to-implementation.md`). A component's code view has the component's name (`MacRow` in Swift, `AndroidRow` in Kotlin) and takes values as the component takes props. Each named row of its `preview.html` (`W('Nested, off', …)`) is a state; its shot is `<platform>.component.<thing>.<state>`: the platform, the name after the platform in lower-case words joined by `-`, and the row's name the same way (`mac.component.row.nested-off`). A row's name is its state's id: renaming a row retires its shot. Screens are checked as the canvas's boards draw them (`design/mocks`).

## Using this system

Load `tokens.css`, then `components/bundle.css`, then `components/bundle.js`, which sets `window.Bae` and needs React 18 on the page. `bundle.js` and `bundle.css` are built from their sources in `components/src/`, one `.js` and one `.css` per area, by `build-bundle.py`; edit the sources, not the bundles. Each folder under `components/` is one component, with its README and its preview, except `components/Cover/`: that is this system's cover, the picture its page shows above everything else, and no component of either app. Android boards also load Roboto from Google Fonts. On a Design canvas, mount components as `<x-import component-from-global-scope="Bae.MacButton" kind="primary">Set up</x-import>`; content inside becomes children, so a `Bae.MacBox` holds `Bae.MacRow`s and a `Bae.MacSheet` holds its content and a `Bae.MacSheetFooter`.

`groups.json` lists every component, and the cover, by what it is for: the groups in order, each with a one-line introduction, the components it holds and its subgroups, each with an introduction and components of its own. The dashboard's Design system tab draws the components in that order, under those introductions. The first line of each component's preview names the component's group (`<!-- @dsCard group="Fields" height=N -->`), and `build-bundle.py` stops with an error when that name differs from the component's group in `groups.json`, or when `groups.json` leaves a component out.

`colors.json` puts every color of `tokens.json` in one group by its role: surfaces, text, the chosen accent, the accents a person can choose, status, lines and overlays, and colors on fills. Each group has a one-line introduction and its swatches, as clusters of colors that belong together or as a table, such as one row per accent and one column per role. A cluster, or a table column, marked `text` draws its swatches as words in the color. `ground` names the color that every swatch is laid over, so a see-through color shows as it does on screen. Each group names a scene, `colors/<scene>.html`: a small drawing in the tokens, in which each element that carries `data-color="<color>"` gets a swatch chip with a line pointing at it (`data-point="below"` points under a word instead of at its left edge). The dashboard's Design system tab draws each group's swatches and its scene in every theme, and shows a problem in the group's place when a color is in no group, in two, or is no color of `tokens.json`.

The canvas draws each `x-import` as the component inside a `<div class="sc-host-x" style="display:contents">`; a preview page draws it bare. Both must draw the same. So a component that reads the components inside it (their type or props) reads `inner(child)` and puts a changed one back with `rewrap(child, changed)` (`src/00-base.js`), and a rule in `bundle.css` that relates two components (`>`, `+`, `:first-child`, `:has(+ …)`) also names the wrapper: `.a > .b, .a > .sc-host-x > .b`. A board drawn with and without the wrapper must place every element the same.
