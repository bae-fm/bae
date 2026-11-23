# Appearance

Every colour and text style the apps draw with is defined once: the tones, the
accents and the text styles in `design/system/tokens.json`, the design system's
own file, and the rest in `design/theme.toml`. The `theme-gen` tool (`bae-theme`) turns them into typed
Swift for BaeKit and Kotlin for Android; `build-macos.sh`, `build-ios.sh` and `build-android.sh` run it, and
its output is not checked in. Both platforms get the same roles, so change
values there rather than adding a platform palette.

Appearance settings have three independent choices: System/Light/Dark mode,
Blue/Orchid/Pink/Lime/Mint/Sky accent, and
Neutral/Slate/Plum/Midnight/Forest/Sand background tone. A new installation
shows System mode, the Pink accent and the Plum tone. The mode default is
each app's own. The accent and tone are the ones `[standard]` in
`design/theme.toml` names (a tone and an accent tokens.json defines), which theme-gen emits as `standard` on the Swift
enums and `STANDARD` on the Kotlin ones; the standard tone also colours
Android's launch window before the app has read the chosen one. A tone is
nine tokens in tokens.json, `tone-<name>-<role>`, each with a light and a dark
surface; theme-gen names the role in lowerCamelCase (`tone-plum-field-hover` is
plum's `fieldHover`). The picker lists the tones in tokens.json's order. Preferences belong to the app
installation rather than a library, so switching libraries preserves the
selection.

Surface roles describe their use: background is the window or screen; surface
and elevated hold content; field and fieldHover hold inputs; missingArt
fills what stands in for content, the plain tile where artwork is missing and
the bars of a list that is still loading; well and tile distinguish recessed
controls from raised controls; control fills push buttons, the tile in dark and
white in light. Accent text, glyphs, and slider fills use the mode-specific accent.
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
warning for what needs attention, and success. Info, uploads and pins have no
colour of their own: an info notice's glyph and the bars, rings and chips of
uploads and pins draw in the chosen accent, and in light an info notice fills
with the accent's tint. Hover, pressed, hairline, scrim, shadow and the
image viewer's backdrop are semantic too, as are onFill and onFillSecondary for
text on fills, artwork and the backdrop. A colour laid behind its own text,
such as a status chip's fill, uses it at the `tint` opacity. The opacities
(`tint`, `tintStrong`, `off`, `dimmed`) come from tokens.json's `opacity`
family. Each token names its role in `role`. The apps draw opacity tokens
that name a role.

Corner radii are roles too, the same on every platform: `artwork` for cover art
in rows and lists, `cover` for a release's cover shown large, `chip`, `control`
for fields, buttons, notices and row highlights, `card`, `panel` for floating
panels and prompts, and `bar` for skeleton and progress bars.

Text styles are roles as well, such as display, hero, title, heading,
rowTitle, strong, body, detail, fine, chip, eyebrow and mono. A role is one text
style of tokens.json per platform that draws it, each naming the role in its
`role`: a style in the `android` family gives Android's size and line height in
sp, and a style in another family gives the macOS size in points, with `ios`
naming the Dynamic Type style iOS follows so text follows the person's text
size. A role's styles share their weight and letter spacing, and a style in the
`mono` family draws in the platform's monospaced face. The design system's
component CSS draws each text style from the same tokens, through the
`--type-<style>-size`, `--type-<style>-weight` and
`--type-<style>-line-height` variables tokens.css gives, and
`scripts/check-design-measures.py` refuses a literal size, weight or line
height there, and a literal padding, margin, gap or corner radius, which
take the spacing and radius tokens. The check reads the inline styles and
`radius` props of the component JavaScript the same way. Apple views set a role with `.themeText(_:)`. A section label is one
`MacEyebrow` on the Mac and one `AndroidEyebrow` on Android.

Spacing is one shared scale named by meaning: hairline, line, inline, compact,
related, fieldInset, group, edge, section and page, and corner radii are named
the same way (bar, chip, artwork, control, field, card, cover and panel). Each
is a token in tokens.json's `spacing` or `radius` family naming its role in
`role`, one length every platform shares. Android's boards also round with
Material 3's extra-small, small and extra-large shapes (`radius-android-*`),
tokens with no `role` that the app takes from Material itself. A corner as
round as its box is `999px`, fully round, whatever the box's size. Glyph sizes (badge, small, medium,
large, hero and the rest) and a few recurring sizes (row and now-playing bar
artwork, the least an icon button takes to press) are roles whose values each
platform sets by its own convention. A role is a token in tokens.json's `icon`
or `size` family, whose value is the macOS length, whose `ios` is iOS's and,
for a glyph, whose `weight` is the one SF Symbols draw it at, and a token in
`icon-android` or `size-android` with Android's length, each naming the role in
its `role`. A glyph role one platform draws alone has only that platform's
token, such as Android's full player transport glyphs (`androidTransport`,
`androidTransportPlay`). A token with no `role`, such as `size-control`, is a
measure only the design system draws. The design system's glyph components take
an icon role as their `size` and draw it at `--icon-<role>` on macOS and
`--icon-android-<role>` on Android; a role the tokens lack stops the component
drawing.

The shared components are defined once per platform: `StatusTone` (neutral,
accent, info, success, warning, danger), a `StatusChip` for short
labels on a tone's fill, a notice (`MacNotice` on the Mac, `AndroidNotice` on
Android) for a message in a tone, `.card(elevated:)` on Apple, and
`MacErrorLine` for a failure line on the Mac. A failing sync cycle is one warning `AndroidNotice` in the Activity sheet on Android.

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
