# MacTransferLine

One transfer in the Activity panel (`MacActivity`): what it does, how far it has got, then a bar.

| Prop | |
| --- | --- |
| `label` | What the transfer does: Syncing changes, Uploading, Pinning, Moving to This Mac. The words also name the bar. |
| `count` | How far it has got, in releases done of the batch ("1 of 3 releases"), or what holds the queue back in its place (Paused, Waiting for network). |
| `progress` | How much of the bytes bae has sent, 0 to 1. Without it the bar moves, as syncing draws it, since syncing counts nothing. |
| `pausable`, `paused` | The line ends in Pause, or Resume while `paused`. Pinning and moving to This Mac are one queue, so both lines' buttons pause it. |

**In the code:** macOS: the private `TransferLine` (`Views/Chrome/ActivityList.swift`).
