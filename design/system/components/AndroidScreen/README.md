# AndroidScreen

Holds `AndroidSection`s over `AndroidCard`s. No `title` on the welcome screen.

`keyboard` draws the keyboard up (`AndroidKeyboard`) and fits the screen, and a dialog over it, in the space above. Every board where a text field has focus draws it: a field is never shown focused with the keyboard down.

**In the code:** Android: `SettingsDestinationScreen` with `SettingsTopBar` (`ui/library/LibraryNavigator.kt`, `ui/settings/SettingsScreen.kt`).
