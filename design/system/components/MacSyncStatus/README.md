# MacSyncStatus

How the library syncs, in Settings: the status in its tone, then what caused it, a countdown while a wait counts down, and a button that fixes it, then when this Mac last synced. With no cause and no fix, the status is a plain `MacRow`. With either, the cause and the button sit under the status in the same part of the box.

| Prop | |
| --- | --- |
| `status`, `tone` | The status, and its tone: `success`, `warning`, `danger`, or none. |
| `cause` | What caused the status and what to do, in core's words, one text. |
| `countdown` | The time left on a wait, under the cause in tabular figures: "Resuming in 42 seconds." |
| `fix` | The primary button that fixes it. |
| `lastSynced` | When this Mac last synced, in a row of its own. |

**In the code:** macOS: `MacSyncStatus` (`bae-macos/bae/bae/Views/Settings/Library/MacSyncStatus.swift`).
