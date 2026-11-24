# MacWelcomeLibraries

The libraries on this Mac, as the welcome window lists them, at most 420pt wide and centred, as the app's welcome column is: a row for each library with its ⋯ menu, then Add library… under the box.

| Prop | |
| --- | --- |
| `state` | `''` (the default): both libraries open. `removing`: bae is leaving the first library, its row shows a spinner and the rest are off. `open-failed`: the first library could not open, and a notice under it offers Details and Reset from cloud…. `unreadable`: a third row for a library bae cannot read, with Remove…. `damaged-join`: a third row for an unfinished join bae cannot read, with Discard…. |

**In the code:** macOS: `MacWelcomeLibraries` (`bae-macos/bae/bae/Views/Welcome/MacWelcomeLibraries.swift`), in `MacWelcomeHome`.
