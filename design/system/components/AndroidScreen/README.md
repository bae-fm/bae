# AndroidScreen

Holds `AndroidSection`s over `AndroidCard`s. No `title` on the welcome screen.

A dialog or sheet placed among the content is drawn over the whole screen: its scrim covers the bars and the now playing bar too, and a dialog centres on the screen. A board never wraps the screen in a box to position it.

`keyboard` draws the keyboard up (`AndroidKeyboard`) and fits the screen, and a dialog over it, in the space above. Every board where a text field has focus draws it: a field is never shown focused with the keyboard down.

**In the code:** Android: `AndroidScreen` (`ui/components/AndroidScreen.kt`), the bar over the screen's own column; `SettingsDestinationScreen` is the navigator's switch over settings destinations, not this.
