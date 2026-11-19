# MacQueueRate

One transfer queue under the Storage Manager's list (`MacStorageFooter`): its arrow, its rate or what holds the whole queue back, and Pause, or Resume while the person has paused it.

| Prop | |
| --- | --- |
| `kind` | `uploads`, `pins` or `exports`. |
| `rate` | How fast the queue goes: 2.4 MB/s. Exports count how far they have got instead, as a percent: 42%. |
| `wait` | What holds the queue back, in place of the rate: Paused, Waiting for network, Waiting for Wi-Fi, Retrying at 6:40 PM. Paused turns Pause into Resume. |

**In the code:** macOS: the queue line in `StorageFooter` (`Views/Storage/StorageFooter.swift`), with `QueuePauseButton`.
