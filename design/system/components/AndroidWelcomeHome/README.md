# AndroidWelcomeHome

The welcome screen's content: the logo over "Your music, everywhere", then `AndroidWelcomeLibraries` and whatever follows it. AndroidWelcomeHome fills an `AndroidScreen`.

| Prop | |
| --- | --- |
| `state` | `AndroidWelcomeLibraries`' state. |
| `empty` | This device has no library yet: a larger logo in the middle of the screen, and `AndroidWelcomeActions` at its bottom. |
| children | What follows the libraries, such as the restore codes saved on this device. |

**In the code:** Android: `AndroidWelcomeHome` (`bae-android/app/src/main/java/fm/bae/app/ui/onboarding/CodeScreen.kt`).
