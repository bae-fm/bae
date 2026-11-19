# MacRestoreCode

The library's restore code in Settings, in a box of its own: hidden but for its first and last characters until the person clicks Show, then in groups of four. Print… and a Copy button sit beside it, and a note under it says what the code is for.

| Prop | |
| --- | --- |
| `shown` | The code starts shown. Show and Hide switch it. |
| `s3` | A second note says that an S3 library also needs its access key and secret. |

**In the code:** macOS: BaeKit `RestoreCodeSection` (`BaeKit/Sources/BaeKit/Views/RestoreCodeSection.swift`).
