# AndroidSnackbar

The app's one snackbar: the line, an action (Retry), the Close button. Place it inside a screen. The action is an `inverse` text `AndroidButton`, and Close is an `inverse` `AndroidIconButton` in a 32dp box. Material caps its line and actions at 600dp, inside 16dp and 8dp sides, and the app centres it. With Close, Material drops the 8dp end side, and Close takes 8dp on each side of its box, so the snackbar is 616dp at most.

**In the code:** Android: `AndroidSnackbar` (`ui/components/AndroidSnackbar.kt`), which `AndroidSnackbarHost`, in the same file, draws each message with.
