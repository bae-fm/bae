# MacTransferReadout

A transfer's bytes and rate over its 4pt bar, filled in the transfer's colour: activity for an upload, pinning for a pin. The bytes moved sit at the start and the rate at the end, in secondary tabular figures; a move to This Mac shows its current file's bytes where the rate goes. AppKit draws the whole readout, because its numbers and bar change many times a second.

| Prop | What it does |
| --- | --- |
| `kind` | `upload` (the default) or `pin`. |
| `bytes` | The bytes moved and the total: 120 MB of 356 MB. |
| `rate` | The rate (6.4 MB/s), or for a move to This Mac the current file's bytes. |
| `progress` | How far the transfer has got, from 0 to 1. |

**In the code:** macOS: `TransferReadoutNSView` (`Views/Storage/TransferReadout.swift`), an `NSView` with two `NSTextField`s and a `ProgressTrackNSView`.
