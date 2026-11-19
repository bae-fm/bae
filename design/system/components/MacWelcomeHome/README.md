# MacWelcomeHome

The welcome window's content: the logo over "Your music, everywhere", then `MacWelcomeLibraries` and whatever follows it. MacWelcomeHome fills a `MacWelcomeWindow`.

| Prop | |
| --- | --- |
| `state` | `MacWelcomeLibraries`' state. |
| `empty` | This Mac has no library yet: a larger logo sits over `MacWelcomeActions`, in place of the list. |
| children | What follows the libraries, such as the restore codes that iCloud Keychain holds. |

**In the code:** macOS: `WelcomeChooseView` (`bae-macos/bae/bae/Views/Welcome/WelcomeChooseView.swift`).
