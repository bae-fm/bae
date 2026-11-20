# AndroidPinControl

On the album page, under the releases: whether this release's files are pinned on this phone, and the one action that changes it, in pinning's words (README, Words): Pin; Queued to pin, with Cancel; Pinning with its bar and bytes, and Cancel pinning; Could not pin, a quiet danger notice with Details (the error in a dialog) and Retry, and Cancel below it; Pinned, with Unpin. On a release this phone can't play, one line saying where it is. Nothing says how many wait ahead or what holds the queue back.

Each button is an outlined `AndroidButton` with its `icon`.

**In the code:** Android: `AndroidPinControl` (`ui/albumdetail/AndroidPinControl.kt`), taking the release's pin status and its progress; `AndroidDownloadControlContainer`, in the same file, reads the session for it. A release this device can't play shows `AndroidUnplayableLine` in its place.
