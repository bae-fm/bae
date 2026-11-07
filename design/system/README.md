bae is a music library app for macOS, Android, iOS, Windows and Linux. Its screens are quiet: dark surfaces in a chosen tone, one accent color, native controls, short words.

Everything here is drawn at each platform's regular size, the one the apps use by default. Other sizes, such as macOS's small and large controls, are variants to add later.

## Words

- Sentence case everywhere on screen: "Set up sync", "Restore code".
- Fewest words that say it. A note is one sentence; most controls need none.
- Buttons are verbs: "Set up", "Cancel", "New folder…". An ellipsis on a button means it opens more before acting; on a waiting line it means work is under way: "Checking the folder “Music”…".
- Placeholders never repeat the label and never end in an ellipsis. Example values are plain and generic: `my-bucket`, `https://s3.home.lan`.
- Error lines say what is wrong and what to do: "“Music” isn’t empty. Choose an empty folder, or make a new one."
- Name no storage company where the choice isn't the person's to make. No exclamation marks, no emoji, no marketing words.

## Color

The apps offer six tones (neutral, slate, plum, midnight, forest, sand), eight accents and light and dark modes. This system holds the midnight tone and the pink accent, in Dark (first) and Light. Each color's use is beside it under Colors.

- Each status carries a word, never color alone.

## Type

The text styles are macOS points. iOS follows Dynamic Type, and the Android components set Android's sizes. No font files ship: each platform uses its own face.

- `title`, `hero` and `display` are for pages and the welcome screen, not settings. `wordmark` is for the word beside the sheep (see Logo).

## Screens

- One primary button per screen, on the right of the footer. Everything else is secondary.
- A sheet's Cancel goes back one screen, and closes the sheet on the first one. No Back buttons.
- Components come per platform: `Mac…` for macOS, `Android…` for Android; `StatusChip` and `Notice` serve both. Windows and Linux follow macOS; iOS will get its own. Each component's guidelines end with its counterpart in the code; "none yet" means the code needs one.
- On macOS, settings are `MacSection` headings over `MacBox`es of `MacRow`s inside a `MacSettingsWindow`. A row that opens something ends in a chevron.
- On Android, a screen is an `AndroidScreen` of `AndroidSection` labels over `AndroidCard`s of `AndroidRow`s; changing a value opens an `AndroidDialog`.
- An empty list is one sentence in the box: "No shared folders yet". No glyph above it.

## Iconography

Icons appear only when they are the control (copy, a chevron, a disclosure mark) or carry meaning the words beside them don't (a status glyph). Apple platforms use SF Symbols and Android Material icons.

## Logo

The sheep is bae's logo: `assets/Logos/bae-sheep.svg`, with no background of its own. It's the product's own artwork, so Iconography's rule doesn't apply to it.

- **Sheep and "bae" together:** where bae introduces itself: Welcome, About and this system's cover. The sheep on top, both centered, the word in the `wordmark` style.
- **Sheep alone:** where the system or the space already says the name: app icons and favicons. On an app icon it sits on a pale pink tile, `#FFD6E7`.
- **Small sheep:** at 32px and under, the full drawing loses its detail, so favicons and small icons use `bae-favicon-16.svg` and `bae-favicon-32.svg`, drawn pixel by pixel for those sizes. Show them at exactly those sizes.
- **"bae" alone:** in sentences, titles and menus, as plain text in the style around it.

`Logo` draws these forms. `AppIcon` draws each platform's icon: the sheep on the pale pink tile on macOS, iOS and Android, and with no tile on Windows and Linux.

## Samples

A component's sample is a set of named rows, each changing one thing.

- Hold the content still. Every row shows the same example with the same words, so the only difference is the state or option the row is named for. No exceptions for a field or a word that seems a better fit.
- One row per state or option, named the way you'd ask for it in code: Resting, Focused, Error, Off.
- When an axis is done, look for the next among the component's other props. Show each value once; don't multiply axes into combinations.
- A container holds a plain stand-in, a dashed box, so the container is the only real thing on show.

## Using this system

Load `tokens.css`, then `components/bundle.css`, then `components/bundle.js`, which sets `window.Bae` and needs React 18 on the page. `bundle.js` is built from its sources in `components/src/`, one file per area, by `build-bundle.py`; edit the sources, not the bundle. Android boards also load Roboto from Google Fonts. On a Design canvas, mount components as `<x-import component-from-global-scope="Bae.MacButton" kind="primary">Set up</x-import>`; content inside becomes children, so a `Bae.MacBox` holds `Bae.MacRow`s and a `Bae.MacSheet` holds its content and a `Bae.MacSheetFooter`.
