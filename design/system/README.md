bae is a music library app for macOS, Android, iOS, Windows and Linux. Its screens are quiet: dark surfaces in a chosen tone, one accent colour, native controls, short words.

## Words

- Sentence case everywhere on screen: "Set up sync", "Restore code".
- Fewest words that say it. A note is one sentence; most controls need none.
- Buttons are verbs: "Set up", "Cancel", "New folder…". An ellipsis on a button means it opens more before acting; on a waiting line it means work is under way: "Checking the folder “Music”…".
- Placeholders never repeat the label and never end in an ellipsis. Example values are plain and generic: `my-bucket`, `https://s3.home.lan`.
- Error lines say what is wrong and what to do: "“Music” isn’t empty. Choose an empty folder, or make a new one."
- Name no storage company where the choice isn't the person's to make. No exclamation marks, no emoji, no marketing words.

## Colour

The apps offer six tones (neutral, slate, plum, midnight, forest, sand), eight accents and light and dark modes. This system holds the midnight tone and the pink accent, in Dark (first) and Light.

- Surfaces by role: `background` is the window; `surface` holds content (boxes of rows, the sidebar); `elevated` floats (sheets); `field` fills text fields; `tile` raises a control; `well` recesses one; `placeholder` stands in for missing artwork.
- Text is `text-primary`, with `text-secondary` for values and notes, on any surface. `text-tertiary` is only for placeholders and unchosen marks.
- `accent` is for accent text, glyphs, the focus ring and a chosen radio. The primary button is `accent-fill` with `on-fill` text, in both modes.
- A colour behind its own text uses its tint (`opacity-tint`): `accent-tint` for a selected row, `success-tint` behind a Synced chip.
- Status: `success` for Synced, `warning` for routine states that need nothing (Offline), `danger` for what broke, `activity` for transfers, `pinning` for files kept on a device. Each status carries a word, never colour alone.
- `hairline` divides rows and edges a box. `scrim` dims the window under a sheet.

## Type

The platform's own face: SF Pro on Apple (`--font-sans`), SF Mono for codes (`--font-mono`), Roboto on Android (`--font-android`, from Google Fonts). No font files ship. The text styles are macOS points; iOS follows Dynamic Type, and the Android components set Android's sizes (rows 16 over 14, notes 12, the top bar's title 22).

- `body` (13) for labels, values and buttons; `small` (12) for text in a field; error lines in `body`; `strong` for a section heading; `heading` (15) for a sheet's title and a pane's heading; `detail` (11.5) for a note under a control; `mono` for a restore code.
- `title`, `hero`, `display` and `wordmark` are for pages and the welcome screen, not settings.

## Space and shape

- Spacing is named by meaning: `space-inline` between a glyph and its text, `space-related` between related items, `space-group` for a row's inset, `space-edge` for a sheet's, `space-section` between sections.
- `radius-control` for fields, buttons, pop-ups, notices and row highlights; `radius-card` for boxes and a sheet's lower corners; `radius-chip` for chips; `radius-bar` for progress bars.
- Controls are `size-control` (22px) tall; rows at least `size-row` (36px). Fields have a fill and no outline.

## Screens

- One primary button per screen, on the right of the footer. Everything else is secondary.
- A sheet's Cancel goes back one screen, and closes the sheet on the first one. No Back buttons.
- Components come per platform: `Mac…` for macOS, `Android…` for Android; `StatusChip` and `Notice` serve both. Windows and Linux follow macOS; iOS will get its own. Each component's guidelines end with its counterpart in the code; "none yet" means the code needs one.
- On macOS, settings are `MacSection` headings over `MacBox`es of `MacRow`s inside a `MacSettingsWindow`. A row that opens something ends in a chevron.
- On Android, a screen is an `AndroidScreen` of `AndroidSection` labels over `AndroidCard`s of `AndroidRow`s; changing a value opens an `AndroidDialog`.
- An empty list is one sentence in the box: "No shared folders yet". No glyph above it.

## Iconography

Icons appear only when they are the control (copy, a chevron, a disclosure mark) or carry meaning the words beside them don't (a status glyph). Apple platforms use SF Symbols and Android Material icons. The macOS components draw the few glyphs they need as inline strokes; the Android ones draw Material's paths (back, edit, chevron, expand, more, search, refresh). `Notice` has no leading glyph.

## Using this system

Load `tokens.css`, then `components/bundle.css`, then `components/bundle.js`, which sets `window.Bae` and needs React 18 on the page. Android boards also load Roboto from Google Fonts. On a Design canvas, mount components as `<x-import component-from-global-scope="Bae.MacButton" kind="primary">Set up</x-import>`; content inside becomes children, so a `Bae.MacBox` holds `Bae.MacRow`s and a `Bae.MacSheet` holds its content and a `Bae.MacSheetFooter`.
