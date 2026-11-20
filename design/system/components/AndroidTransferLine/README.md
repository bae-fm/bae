# AndroidTransferLine

One transfer in Android's Activity sheet (`AndroidActivity`), as `MacTransferLine` on Mac: what it does, how far it has got, then Material's bar.

| Prop | |
| --- | --- |
| `label` | What the transfer does: Syncing changes, Uploading, Pinning. The words also name the bar. |
| `count` | Releases done of the batch, or what holds the queue back in its place. |
| `progress` | 0 to 1. Without it the bar moves. |
| `pausable`, `paused` | A tonal Pause, or Resume while `paused`. Only Pinning has one: a phone never uploads release files. |
| `opens` | The line opens a screen when tapped: Pinning opens the Pinning screen. |

**In the code:** Android: `AndroidTransferLine` (`ui/sync/ActivityTransfers.kt`).
