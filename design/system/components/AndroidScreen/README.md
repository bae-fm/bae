# AndroidScreen

An Android screen: the top bar on `surface` with Back and the title in `title`, then the content on `background`.

- Props: `title` (none: no top bar, as on the welcome screen), `back`, `width` and `height` (412 × 916).
- Children are the content: `AndroidSection`s over `AndroidCard`s. An `AndroidDialog` inside covers the whole screen.

**In the code:** Android: `SettingsDestinationScreen` with `SettingsTopBar` (`ui/library/LibraryNavigator.kt`, `ui/settings/SettingsScreen.kt`).
