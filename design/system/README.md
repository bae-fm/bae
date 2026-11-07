bae is a music library app for macOS, Android, iOS, Windows and Linux. Its screens are quiet: dark surfaces in a chosen tone, one accent color, native controls, short words.

Everything here is drawn at each platform's regular size, the one the apps use by default. Other sizes, such as macOS's small and large controls, are variants to add later.

## Words

- Sentence case everywhere on screen: "Set up sync", "Restore code".
- Fewest words that say it. A note is one sentence; most controls need none.
- A full sentence ends in a period: "Open Settings on your desktop to add this device." A label or fragment has none: "No shared folders yet".
- Buttons are verbs: "Set up", "Cancel", "New folder…". An ellipsis on a button means it opens more before acting; on a waiting line it means work is under way: "Checking the folder “Music”…".
- Placeholders never repeat the label and never end in an ellipsis. Example values are plain and generic: `my-bucket`, `https://s3.home.lan`.
- Error lines say what is wrong and what to do: "“Music” isn’t empty. Choose an empty folder, or make a new one."
- Name no storage company where the choice isn't the person's to make. No exclamation marks, no emoji, no marketing words.

## Color

The apps offer six tones (neutral, slate, plum, midnight, forest, sand), eight accents and light and dark modes. This system holds the midnight tone and the pink accent, the ones a new installation starts with, in Dark (first) and Light. Each color's use is beside it under Colors.

- The accents are measured in OKLCH, a color space where equal steps in the numbers look equal. Each fill has the sheep's pink lightness, 0.75, and its saturation, 0.17, or the most the screen can show at that hue. The hues start at the sheep's pink (353°), and the other seven sit where each is the same visible distance from its neighbours: blue 268°, orchid 313°, pink 353°, coral 36°, gold 81°, lime 123°, mint 166°, sky 213°.
- Text on an accent fill is the same hue at lightness 0.28 and saturation 0.09.
- An accent as text and glyphs (`accent-blue` and the like) is the hue at lightness 0.78 and saturation 0.11 in Dark, and at lightness 0.53 and saturation 0.16 in Light, made darker where needed to reach 4.5:1 on background, surface and elevated. A selected row is the Light text color in both modes, with white text, quieter than the button.
- How the color names read: a bare name (`accent`, `accent-blue`, `danger`, `info`) is text and glyphs on background, surface and elevated. `-fill` is a filled shape, and `on-…-fill` is what sits on it. `-selection` is a selected row, and `-tint` a faint wash behind something chosen.
- `accent`, `accent-fill`, `on-accent-fill` and `accent-selection` are the chosen accent; the boards show pink. Each accent's own set is beside it under Colors.
- Each status carries a word, never color alone.

## Type

The text styles are macOS points. iOS follows Dynamic Type, and the Android components set Android's sizes. No font files ship: each platform uses its own face.

- `title`, `hero` and `display` are for pages and the welcome screen, not settings.

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

- **Sheep and "bae" together:** where bae introduces itself: Welcome, About and this system's cover. The sheep on top, both centered, the word `assets/Logos/bae-word.svg` at 0.52 of the sheep's width. The word is Fredoka SemiBold turned into a drawing, so it's the same on every platform and no app needs the font. Fredoka is for the logo only.
- **Sheep and "bae" in a line:** where there's width but little height, such as a header. The sheep comes first: on the left, or on the right in a right-to-left language. The word is 0.8 of the sheep's width.
- **The tagline:** "Your music, everywhere". It sits under the logo on Welcome, in secondary text with no period, and stays still while the logo floats.
- **The bob:** on Welcome the sheep and the word float separately, the rise and the tilt on timers of their own so the motion doesn't visibly repeat. Each rises by 4% of the sheep's height every 6 seconds and tilts 2° each way every 7.3, the word 2 seconds behind the sheep. It stops when the person has asked for reduced motion.
- **The glow:** on Welcome a pink and purple disc of light sits still behind the sheep and the word, 2.6 times the sheep's width, in the website's colors, fading out with no visible edge, with a pool of the same light under the sheep. The word is the sheep's cream, `sheep-cream`, with a soft white 1px edge to the lower right, at 45°.
- **Sheep alone:** where the system or the space already says the name: app icons and favicons. On an app icon it sits on a pale pink tile, `#FFD6E7`.
- **Small sheep:** at 32px and under, the full drawing loses its detail, so favicons and small icons use `bae-favicon-16.svg` and `bae-favicon-32.svg`, drawn pixel by pixel for those sizes. Show them at exactly those sizes.
- **"bae" alone:** the drawn word, `assets/Logos/bae-word.svg`, where a logo belongs but the sheep doesn't fit.
- **"bae" in text:** in sentences, titles and menus the name is plain text in whatever font is around it, never the drawn word.

`Logo` draws these forms. `AppIcon` draws each platform's icon: the sheep on the pale pink tile on macOS, iOS and Android, and with no tile on Windows and Linux.

## Samples

A component's sample is a set of named rows, each changing one thing.

- Hold the content still. Every row shows the same example with the same words, so the only difference is the state or option the row is named for. No exceptions for a field or a word that seems a better fit.
- One row per state or option, named the way you'd ask for it in code: Resting, Focused, Error, Off.
- When an axis is done, look for the next among the component's other props. Show each value once; don't multiply axes into combinations.
- A container holds a plain stand-in, a dashed box, so the container is the only real thing on show.

## Using this system

Load `tokens.css`, then `components/bundle.css`, then `components/bundle.js`, which sets `window.Bae` and needs React 18 on the page. `bundle.js` is built from its sources in `components/src/`, one file per area, by `build-bundle.py`; edit the sources, not the bundle. Android boards also load Roboto from Google Fonts. On a Design canvas, mount components as `<x-import component-from-global-scope="Bae.MacButton" kind="primary">Set up</x-import>`; content inside becomes children, so a `Bae.MacBox` holds `Bae.MacRow`s and a `Bae.MacSheet` holds its content and a `Bae.MacSheetFooter`.
