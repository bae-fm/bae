# AndroidWelcomeLibraries

The libraries on this device, as the welcome screen lists them: a row for each library with its ⋯ menu, and Add library as the card's last row.

| Prop | |
| --- | --- |
| `state` | `''` (the default), `removing`, `unreadable` or `damaged-join`, as `MacWelcomeLibraries` draws them. |

While a library is removed, its row shows the removal's step, such as Leaving the library…, as the row's second line in secondary text, with a spinner where the ⋮ was.

**In the code:** Android: `AndroidWelcomeLibraries` (`bae-android/app/src/main/java/fm/bae/app/ui/onboarding/AndroidWelcomeLibraries.kt`).
