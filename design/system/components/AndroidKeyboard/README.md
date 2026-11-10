# AndroidKeyboard

The on-screen keyboard, up while a text field has focus: three rows of letters, the bottom row with the space bar, the gesture bar under it. Drawn plainly, so a board shows how much of the screen is left, not any one keyboard app. Boards don't place it themselves: `AndroidScreen` with `keyboard` puts it at the bottom and fits the screen above it.

**In the code:** Android: the system's keyboard. Compose screens with a focused field use `imePadding()` so their content, and a dialog, stays above it.
