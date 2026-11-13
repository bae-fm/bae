bae is a music library app for macOS, Android, iOS, Windows and Linux. Its screens are quiet: dark surfaces in a chosen tone, one accent color, native controls, few words.

Everything here is drawn at each platform's regular size, the one the apps use by default. Other sizes, such as macOS's small and large controls, are variants to add later.

## Words

- Sentence case everywhere on screen: "Set up sync", "Restore code".
- Minimal words, phrases and sentences, and only to say what the UI itself can't. A note is one sentence; most controls need none.
- A full sentence ends in a period: "People in this library see this name." A label or fragment has none: "No shared folders yet".
- One-line text that doesn't fit is cut with "…": a file or folder name in the middle, so its start and its end both show (`Live at the Ha…1994-03-12.flac`); a title, a name and a sentence at the end, since they read from the start ("The import could not be…"). The whole text is where the row opens.
- Buttons are verbs: "Set up", "Cancel", "New folder…". An ellipsis on a button means it opens more before acting; on a waiting line it means work is under way: "Checking the folder…".
- Placeholders never repeat the label and never end in an ellipsis. Example values are plain and generic: `my-bucket`, `https://s3.home.lan`.
- A notice says what happened, what's wrong and the fix, each once: where it sits names its subject; the title is what happened in a word or two; the line is what's wrong in words any user understands, with no mechanism and no claim the code doesn't back; a button is the fix, named for what it does, never a pointer to a control elsewhere; the fix's cost is said in the confirm it opens; the technical cause is behind Details. `MacNotice`'s README walks through one.
- No quotes around names or the names of controls: where a thing sits, or the control itself, says which.
- The person's cloud storage is **the cloud** in everyday words, on every platform and screen: "In cloud", the filter "Cloud", "Move to cloud", "Cloud full", "Could not reach the cloud", "The library will remain in the cloud". Where sync is set up, the place itself (the folder or bucket in their Dropbox, OneDrive, iCloud or S3 service that the library lives in, coven's *store* on its *provider*) is their **cloud home**: the Sync settings section headed "Cloud home", "Choose a cloud home", setup's steps. Never "storage" alone for either: people can't tell which storage. Storage Manager and a release's Storage sheet keep "Storage", since they cover the files on this device too, and so does a phone's own storage. **For implementers:** the apps' strings, every locale's catalog and the spec's user-visible words still say "storage" in many places; replace each one that means the cloud with "cloud" (or "cloud home" in sync setup), and leave the Storage Manager's and the device's own.
- Keeping a release's files on a device is pinning, on every platform and every screen: the action is Pin; under way, Pinning; done, Pinned; undone, Unpin; stopped while under way, Cancel pinning; waiting, Queued to pin; failed, Could not pin. Never "download", "keep here", "copying here" or "for offline" for it. The screen of pins on Android is Pinning; its settings are Pins at once, Pause pinning and Pin over Wi-Fi only. Moving a release to This Mac is a different thing with its own words, named for the device as the import's destination names it (This Mac or Cloud): Move to This Mac…; under way, Moving to This Mac…; waiting, Queued to move to This Mac; failed, Could not move to This Mac. Never "make local" for it, and never counted, shown or named as pinning: a pin keeps a copy of a cloud file in this device's cache and changes nothing elsewhere, while moving to This Mac changes where the file lives, for every device. On desktop the two share one download queue, so they contend for the same downloads; what covers both says downloads: Pause downloads, Downloads at once, the footer's ↓ rate.
- A waiting state names what it waits to do: Queued to pin, Queued to upload, Queued to move to This Mac, never a bare Queued.
- A release's storage shows its state and at most one pill for the change on its way; nothing says how many wait ahead of it. What holds a whole queue back (Paused, Waiting for network, Waiting for Wi-Fi, Retrying at…) shows once, in words, where the queue shows: the Storage Manager's footer, the Activity panel's transfer lines, Android's Pinning screen. A release's own problem, such as a missing original that stops its upload, shows on the release.
- Name no storage company where the choice isn't the person's to make. No exclamation marks, no emoji, no marketing words.

## Color

The apps offer six tones (neutral, slate, plum, midnight, forest, sand), eight accents and light and dark modes. This system holds the midnight tone and the pink accent, the ones a new installation starts with, in Dark (first) and Light. Each color's use is beside it under Colors.

- The accents are measured in OKLCH, a color space where equal steps in the numbers look equal. Each fill has the sheep's pink lightness, 0.75, and its saturation, 0.17, or the most the screen can show at that hue. The hues start at the sheep's pink (353°), and the other seven sit where each is the same visible distance from its neighbours: blue 268°, orchid 313°, pink 353°, coral 36°, gold 81°, lime 123°, mint 166°, sky 213°.
- Text on an accent fill is the same hue at lightness 0.28 and saturation 0.09.
- An accent as text and glyphs (`accent-blue` and the like) is the hue at lightness 0.78 and saturation 0.11 in Dark, and at lightness 0.53 and saturation 0.16 in Light, made darker where needed to reach 4.5:1 on background, surface and elevated. A selected row is the Light text color in both modes, with white text, quieter than the button.
- How the color names read: a bare name (`accent`, `accent-blue`, `danger`, `info`) is text and glyphs on background, surface and elevated. `-fill` is a filled shape, and `on-…-fill` is what sits on it. `-selection` is a selected row, and `-tint` a faint wash behind something chosen.
- `accent`, `accent-fill`, `on-accent-fill` and `accent-selection` are the chosen accent; the boards show pink. Each accent's own set is beside it under Colors.
- Each status carries a word, never color alone.
- Secondary text on macOS is the system's own colour (`mac-text-secondary`, measured from NSColor.secondaryLabelColor), which follows Increase Contrast and translucent backgrounds there; the Mac code uses it directly. `text-secondary` is Android's. Every Mac window draws the system colour, and a Mac part board or preview outside a window is marked `bae-mac`.

## Type

The text styles are macOS points. iOS follows Dynamic Type, and the Android components set Android's sizes. No font files ship: each platform uses its own face.

- `title`, `hero` and `display` are for pages and the welcome screen, not settings.

## Screens

- One primary button per screen, on the right of the footer. Everything else is secondary.
- A sheet's Cancel goes back one screen, and closes the sheet on the first one. No Back buttons.
- Components come per platform: `Mac…` for macOS, `Android…` for Android; `StatusChip` serves both. Windows and Linux follow macOS; iOS will get its own. Each component's guidelines end with its counterpart in the code; "none yet" means the code needs one.
- A wait with no length shows each platform's own spinner: macOS's spokes (`MacSpinner`), Material's ring (`AndroidSpinner`). A spinner is a signal people already read as busy and carries nothing of bae's, so it is not made the same across platforms.
- Every colour on a platform comes from the design system: the accent the person chose, the surfaces, the text. None is left to the system's own colour (the Mac's accent, Material's dynamic colour) beside them; a part the platform colours with no way to change it is drawn by the app.
- The boards are the target: where a board and an app differ, the app changes. A board's words, sizes, names and colours are never copied from an app.
- On macOS, settings are `MacSection` headings over `MacBox`es of `MacRow`s inside a `MacSettingsWindow`. A row that opens something ends in a chevron.
- On Android, a screen is an `AndroidScreen` of `AndroidSection` labels over `AndroidCard`s of `AndroidRow`s; changing a value opens an `AndroidDialog`.
- An empty list is one sentence in the box: "No shared folders yet". No glyph above it.
- Text in a block shares one left edge. Markers (a status glyph, a disclosure chevron, a bullet) hang in the margin to its left and never push the text in.
- A row with a leading icon keeps it in the leading column, `size-lead` wide, centred, the text starting `space-related` after it on macOS (`space-edge` on Android, as Material's list items). On macOS every such row is built from `MacItem`, and so is `MacNotice`'s glyph, so wherever a notice sits among rows, their words share one edge: the Storage Manager's inspector, the Activity panel. An empty leading column stays, so a row without an icon keeps the edge too. The shared edge holds within one group (a box on macOS, a section on Android) and never runs across groups: in the Activity panel the problems' box keeps it, and Transfers starts at its own box's inset.
- A marker hangs only into a margin its own block owns, such as a notice's glyph column. Where the text edge is a container's edge, there is no margin to hang into: the marker goes inside, and a disclosure becomes its own row with the chevron inside it.
- Dialogs, sheets and alerts, by what they ask of the person:
  - **A decision** (confirm, choose, acknowledge an error): macOS `MacAlert`; Android `AndroidDialog`; iOS an alert, or a confirmation dialog for a destructive choice.
  - **Something to look at, nothing to decide** (a library's details, the queue, the cast picker, the Activity list): Android `AndroidSheet`, closed by swiping down or tapping outside, with no Close button; iOS a sheet at its medium height; macOS a popover hanging off what it describes.
  - **A task with fields or steps** (invite, add a device, set up sync): macOS a sheet on the window (`MacModal`); Android a dialog when it is one or two fields, its own screen when it is more; iOS a sheet with Cancel and Done, since iOS puts forms in sheets.
- Three ways to fold things away, each for its own job:
  - **A disclosure row** folds part of one group: Advanced settings, a short list most people don't need. Every platform has one, in its own density. macOS: `MacDisclosure`, its own box, the chevron leading inside the row, pointing right and turning down when open, the rows it shows indented to the label. Android: `AndroidDisclosure`, a row in its own card, `expand_more` at the end when closed and `expand_less` when open, the rows it shows joining the card on the same edge.
  - **A collapsible section** folds a whole section of a long screen: its heading opens and closes it. Android only, the section label with `expand_more` and `expand_less` at its end. macOS has no collapsible section for content; there it is a disclosure row. No screen needs one yet.
  - **A sidebar heading** folds a group of destinations in a macOS sidebar, its chevron showing only under the pointer. It is navigation, not content. bae has no sectioned sidebar yet.
- A disclosure's chevron on macOS points right closed and down open, leading inside its row (`MacDisclosure`).
- A notice never shows the error itself. When there is an error behind it, its Details button opens an alert (`MacAlert` with `details`) or dialog (`AndroidDialog` holding `AndroidErrorDetails`) with the explanation and the causes, one a line, with Copy. In the Activity panel, clicking (tapping) the problem opens it instead.
- Inside an alert or dialog a notice is always `quiet` (no fill, the glyph in its tone): a filled box in a dialog stands out from everything else in it. "Unsent changes will be lost" in the removal confirmation.
- An error in part of a screen is its box; a Retry that belongs to it is the box's action, at its right, level with the message.
- A view that failed whole is a notice in the centred layout, quiet, a small grey outline glyph before the message, then Details and Retry on one row under it, Retry last. Details opens the causes in a popup, as every notice's does.

## Iconography

Icons appear only when they are the control (copy, a chevron, a disclosure mark) or carry meaning the words beside them don't (a status glyph). Android uses Material Symbols (Rounded, weight 400), loaded from Google Fonts. Apple platforms use SF Symbols; the boards draw stand-ins, since SF Symbols can't be loaded on a web page.

A status glyph is a ring with its mark inside (`StatusGlyph`): info `i`, success a check, warning and danger `!`, told apart by color and by the words beside them. It has no filled form, so it stays quieter than the words wherever it shows. No octagons, no triangles: nothing that reads as a road sign.

## Logo

The sheep is bae's logo: `assets/Logos/bae-sheep.svg`, with no background of its own. It's the product's own artwork, so Iconography's rule doesn't apply to it.

- **Sheep and "bae" together:** where bae introduces itself: Welcome, About and this system's cover. The sheep on top, both centered, the word `assets/Logos/bae-word.svg` at 0.52 of the sheep's width. The word is Fredoka SemiBold turned into a drawing, so it's the same on every platform and no app needs the font. Fredoka is for the logo only.
- **Sheep and "bae" in a line:** where there's width but little height, such as a header. The sheep comes first: on the left, or on the right in a right-to-left language. The word is 0.8 of the sheep's width.
- **The tagline:** "Your music, everywhere". It sits under the logo on Welcome, in secondary text with no period, and stays still while the logo floats.
- **The bob:** on Welcome the sheep and the word float separately, the rise and the tilt on timers of their own so the motion doesn't visibly repeat. Each rises by 4% of the sheep's height every 6 seconds and tilts 2° each way every 7.3, the word 2 seconds behind the sheep. It stops when the person has asked for reduced motion.
- **The glow:** on Welcome a pink and purple disc of light sits still behind the sheep and the word, 2.6 times the sheep's width, in the website's colors, fading out with no visible edge, with a pool of the same light under the sheep. The word is `logo-word`, the wool's cream in dark mode and the outline's plum in light mode, with a soft white 1px edge to the lower right, at 45°.
- **Sheep alone:** where the system or the space already says the name: app icons and favicons. On an app icon it sits on a pale pink tile, `#FFD6E7`.
- **The sheep at play:** on the empty Import screen the sheep, alone, plays a short moment every few seconds, most of them taking in made-up releases (`ImportSheep`). It is the logo's own drawing moved in parts, never mirrored, and holds still when the person has asked for reduced motion.
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

## Shots

The code is checked against this system in two kinds of shot (agent-instructions `principles/design-to-implementation.md`). A component's code view has the component's name (`MacRow` in Swift, `AndroidRow` in Kotlin) and takes values as the component takes props. Each named row of its `preview.html` (`W('Nested, off', …)`) is a state; its shot is `<platform>.component.<thing>.<state>`: the platform, the name after the platform in lower-case words joined by `-`, and the row's name the same way (`mac.component.row.nested-off`). A row's name is its state's id: renaming a row retires its shot. Screens are checked as the canvas's boards draw them (`design/mocks`).

## Using this system

Load `tokens.css`, then `components/bundle.css`, then `components/bundle.js`, which sets `window.Bae` and needs React 18 on the page. `bundle.js` and `bundle.css` are built from their sources in `components/src/`, one `.js` and one `.css` per area, by `build-bundle.py`; edit the sources, not the bundles. Each folder under `components/` is one part, its README and its preview, except `components/Cover/`: that is this system's cover, the picture its page shows above everything else, and no part of either app. Android boards also load Roboto from Google Fonts. On a Design canvas, mount components as `<x-import component-from-global-scope="Bae.MacButton" kind="primary">Set up</x-import>`; content inside becomes children, so a `Bae.MacBox` holds `Bae.MacRow`s and a `Bae.MacSheet` holds its content and a `Bae.MacSheetFooter`.

The canvas draws each `x-import` as the component inside a `<div class="sc-host-x" style="display:contents">`; a preview page draws it bare. Both must draw the same. So a part that reads the parts inside it (their type or props) reads `inner(child)` and puts a changed one back with `rewrap(child, changed)` (`src/00-base.js`), and a rule in `bundle.css` that relates two parts (`>`, `+`, `:first-child`, `:has(+ …)`) also names the wrapper: `.a > .b, .a > .sc-host-x > .b`. A board drawn with and without the wrapper must place every element the same.
