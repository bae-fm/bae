# Appearance

Every colour the apps draw with is defined once, in `design/theme.toml`. The
`theme-gen` tool (`bae-theme`) turns it into typed Swift for BaeKit and Kotlin
for Android; `build-macos.sh`, `build-ios.sh` and `build-android.sh` run it, and
its output is not checked in. Both platforms get the same roles, so change
values there rather than adding a platform palette.

Appearance settings have three independent choices: System/Light/Dark mode,
Blue/Indigo/Purple/Pink/Red/Amber/Green/Teal accent, and
Neutral/Slate/Plum/Midnight/Forest/Sand background tone. Defaults are System,
Blue, and Neutral. Each tone defines both light and dark surfaces. Preferences
belong to the app installation rather than a library, so switching libraries
preserves the selection.

Surface roles describe their use: background is the window or screen; surface
and elevated hold content; field and fieldHover hold inputs; placeholder holds
missing artwork; well and tile distinguish recessed controls from raised
controls. Accent text, glyphs, and slider fills use the mode-specific accent.
Primary buttons use the separate fill color with white text to preserve
contrast in both modes. The macOS mode selector uses that fill for its selected
segment; iOS retains its native neutral segmented control.
Semantic colours do not change with the tone or accent: danger for errors,
warning for what needs attention, success, info, and activity for work in
progress such as a transfer. Hover, pressed, hairline, scrim, shadow and the
image viewer's backdrop are semantic too, as are onFill and onFillSecondary for
text on fills, artwork and the backdrop. A colour laid behind its own text,
such as a status chip's fill, uses it at the `tint` opacity.

Apple views use `Theme`, `ThemeOpacity`, `PrimaryButtonStyle`, and
`.appAppearance()` at every scene root; Apple's hierarchical label styles stay
native. Native controls retain their platform geometry and interaction.
Android maps the theme to Material colors, reads the other roles through
`BaeTheme.colors` and `BaeTheme.surfaces`, and uses `PrimaryButton`, with tonal
elevation disabled so surfaces retain the selected tone. Navigation
and transport controls use neutral surfaces; selection and progress use the
accent. Action buttons do not add accent shadows or decorative gradients.

The screenshot suites render production views in light and dark modes and all
six tones. Preference tests exercise persistence and refused writes; Android
also tests concurrent choices and cancellation during an accepted write.
