# AndroidScreen

Holds `AndroidSection`s over `AndroidCard`s. No `title` on the welcome screen.

`keyboard` draws the keyboard up (`AndroidKeyboard`) and fits the screen, and a dialog over it, in the space above. Every board where a text field has focus draws it: a field is never shown focused with the keyboard down.

**In the code:** Android: each screen lays out `ScreenTopBar` (`ui/components/ScreenTopBar.kt`) over its own column (`ui/settings/*Screen.kt`); no one view. (`SettingsDestinationScreen` is the navigator's switch over settings destinations, not this.)
