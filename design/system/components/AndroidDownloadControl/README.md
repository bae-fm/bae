# AndroidDownloadControl

On the album page, under the releases: whether this release's files are pinned on this phone, and the one action that changes it, in pinning's words (README, Words): Pin; Queued to pin, with Cancel; Pinning with its bar and bytes, and Cancel pinning; Couldn’t pin, a quiet danger notice with Details (the error in a dialog) and Retry, and Cancel below it; Pinned, with Unpin. On a release this phone can't play, one line saying where it is. Nothing says how many wait ahead or what holds the queue back.

**In the code:** Android: `AndroidDownloadControl` (`ui/albumdetail/AndroidDownloadControl.kt`), taking the release's download status and its progress; `AndroidDownloadControlContainer`, in the same file, reads the session for it. A release this device can't play shows `UnplayableLine` in its place.
