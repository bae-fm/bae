# AndroidPinControl

On the album page, under the releases: a pin of this release that is under way or has failed, with the one action that changes it, in pinning's words (README, Words): Queued to pin, with Cancel; Pinning with its bar and bytes, and Cancel pinning; Could not pin, a quiet danger notice with Details (the error in a dialog) and Retry, and Cancel below it. Pin and Unpin are items of the album's ⋮ menu. Nothing says how many wait ahead or what holds the queue back.

Each button is an outlined `AndroidButton` with its `icon`.

**In the code:** Android: `AndroidPinControl` (`ui/albumdetail/AndroidPinControl.kt`), taking the release's pin status and its progress; `AndroidDownloadControlContainer`, in the same file, reads the session for it.
