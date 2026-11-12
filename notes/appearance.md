# Appearance

Every colour the apps draw with is defined once: the accents in
`design/system/tokens.json`, the design system's own file, and the rest in
`design/theme.toml`. The `theme-gen` tool (`bae-theme`) turns them into typed
Swift for BaeKit and Kotlin for Android; `build-macos.sh`, `build-ios.sh` and `build-android.sh` run it, and
its output is not checked in. Both platforms get the same roles, so change
values there rather than adding a platform palette.

Appearance settings have three independent choices: System/Light/Dark mode,
Blue/Orchid/Pink/Coral/Gold/Lime/Mint/Sky accent, and
Neutral/Slate/Plum/Midnight/Forest/Sand background tone. A new installation
shows System mode, the Pink accent and the Plum tone. The mode default is
each app's own. The accent and tone are the ones `[standard]` in
`design/theme.toml` names (the accent is one tokens.json defines), which theme-gen emits as `standard` on the Swift
enums and `STANDARD` on the Kotlin ones; the standard tone also colours
Android's launch window before the app has read the chosen one. Each tone
defines both light and dark surfaces. Preferences belong to the app
installation rather than a library, so switching libraries preserves the
selection.

Surface roles describe their use: background is the window or screen; surface
and elevated hold content; field and fieldHover hold inputs; placeholder holds
missing artwork; well and tile distinguish recessed controls from raised
controls. Accent text, glyphs, and slider fills use the mode-specific accent.
An accent is four tokens in tokens.json: `accent-<name>` (text and glyphs, per
appearance), `accent-<name>-fill` with `on-accent-<name>-fill` (its swatch in
the picker and the check drawn on it) and `accent-<name>-selection`. The
swatch, its check and the selection are one colour in both appearances, and
theme-gen refuses a tokens.json where they differ. Primary buttons use the
selection colour with white text to preserve contrast in both modes. The macOS
mode selector uses it for its selected segment. The picker lists the accents in
tokens.json's order. A stored accent the apps no longer have reads as the
standard one.
The Mac declares the standard accent's selection colour as its AccentColor
asset (`bae-theme`'s tests hold it to tokens.json), which macOS draws its own
parts in, such as a menu's highlighted item, while the system accent is
Multicolor. macOS cannot change that colour while the app runs, so what the
person's chosen accent must colour, such as the chosen Settings pane, is drawn
by the app.
Semantic colours do not change with the tone or accent: danger for errors,
warning for what needs attention, success, info, activity for work in
progress such as a transfer, an upload among them, and pinning for files
downloading to stay on a device. Hover, pressed, hairline, scrim, shadow and the
image viewer's backdrop are semantic too, as are onFill and onFillSecondary for
text on fills, artwork and the backdrop. A colour laid behind its own text,
such as a status chip's fill, uses it at the `tint` opacity.

Corner radii are roles too, the same on every platform: `artwork` for cover art
in rows and lists, `cover` for a release's cover shown large, `chip`, `control`
for fields, buttons, notices and row highlights, `card`, `panel` for floating
panels and prompts, and `bar` for skeleton and progress bars.

Text styles are roles as well: display, hero, title, heading, rowTitle, strong,
body, detail, fine, chip, eyebrow and mono.
Weight, letter spacing, case and monospacing are shared; each platform has its
own size, and iOS names a Dynamic Type style so text follows the person's text
size. Apple views set a role with `.themeText(_:)`, and the section labels on
every platform are one `Eyebrow`.

Spacing is one shared scale named by meaning: hairline, line, inline, compact,
related, group, edge, section and page. Glyph sizes (badge, small, medium,
large, hero) and a few recurring sizes (row and now-playing bar artwork, the
least an icon button takes to press) are roles whose values each platform sets
by its own convention.

The shared components are defined once per platform: `StatusTone` (neutral,
accent, info, success, warning, danger, activity), a `StatusChip` for short
labels on a tone's fill, a notice (`Notice` on both)
for a message in a tone, `.card(elevated:)` on Apple, and `ErrorText` for a
failure line. A failing sync cycle is one warning `ErrorNotice` in the Activity sheet on Android.

Apple views use `Theme`, `ThemeOpacity`, `PrimaryButtonStyle`, and
`.appAppearance()` at every scene root; Apple's hierarchical label styles stay
native. Native controls retain their platform geometry and interaction.
Android maps the theme to Material colors, reads the other roles through
`BaeTheme.colors` and `BaeTheme.surfaces`, and uses `AndroidButton`, with tonal
elevation disabled so surfaces retain the selected tone. Navigation
and transport controls use neutral surfaces; selection and progress use the
accent. Action buttons do not add accent shadows or decorative gradients.

The screenshot suites render production views in light and dark modes and all
six tones. The macOS capture of the stock look, dark in the standard tone and
accent, carries no suffix. Preference tests exercise persistence and refused writes; Android
also tests concurrent choices and cancellation during an accepted write.
