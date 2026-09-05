# Appearance

The four apps bundle `BaeKit/Sources/BaeKit/Resources/AppearancePalette.json`.
BaeKit reads it for macOS and iOS; Avalonia embeds that file as a resource;
Android copies it into generated raw resources during its build. Change color
values there rather than adding a platform palette.

Appearance settings have three independent choices: System/Light/Dark mode,
Blue/Indigo/Purple/Pink/Red/Amber/Green/Teal accent, and
Neutral/Slate/Plum/Midnight/Forest/Sand background tone. Defaults are System,
Blue, and Neutral. Each tone defines both light and dark surfaces. Preferences belong to the app installation rather than
a library, so switching libraries preserves the selection.

Surface roles describe their use: background is the window or screen; surface
and elevated hold content; field and fieldHover hold inputs; placeholder holds
missing artwork; well and tile distinguish recessed controls from raised
controls. Accent text, glyphs, and slider fills use the mode-specific accent.
Primary buttons use the separate fill color with white text to preserve
contrast in both modes. The macOS mode selector uses that fill for its selected
segment; iOS retains its native neutral segmented control.
Semantic warning and destructive colors do not change with the chosen accent.

Apple views use `Theme`, `PrimaryButtonStyle`, and `.appAppearance()` at every
scene root. Native controls retain their platform geometry and interaction.
Avalonia maps the palette to dynamic resources and Fluent colors, with a flat
primary button style. Android maps it to Material colors and `PrimaryButton`,
with tonal elevation disabled so surfaces retain the selected tone. Navigation
and transport controls use neutral surfaces; selection and progress use the
accent. Action buttons do not add accent shadows or decorative gradients.

The screenshot suites render production views in light and dark modes and all
six tones. Avalonia captures accept `--capture-variant`, `--capture-tone`, and
`--capture-accent`. Its appearance tests also verify live resource updates and
text contrast across every mode, tone, and accent combination. Preference tests
exercise persistence and refused writes; Android also tests concurrent choices
and cancellation during an accepted write.
