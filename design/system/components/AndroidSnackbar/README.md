# AndroidSnackbar

The app's one snackbar: the line, an action (Retry), the close X. Place it inside a screen. The action is an `inverse` text `AndroidButton`. Material caps its line and actions at 600dp, inside 16dp and 8dp sides, and the app centres it.

**In the code:** Android: `AndroidSnackbar` (`ui/components/AndroidSnackbar.kt`), which `AndroidSnackbarHost`, in the same file, draws each message with.
