# User Journey

## Stage 1: Import & play

You download bae, create a library, import music from folders. MusicBrainz matches provide metadata and cover art. You browse, play, and organize.

Everything is local to the machine.

## Stage 2: Sync across devices

You get a second machine or want cloud backup. Sign in with a commercial cloud provider or configure an S3-compatible bucket. This creates your cloud home -- one location that holds everything.

Consumer clouds are the primary path (OAuth sign-in). S3-compatible providers (Backblaze B2, Cloudflare R2, AWS, Wasabi, etc.) are for more technical users.

Your library syncs incrementally via changesets. One library, your own devices.

## Stage 3: Connect another device

You set up bae on another of your machines. Instead of creating a new library, you connect it to the one you already have. On the first machine you generate a **restore code**; you paste it into bae on the new machine.

The restore code carries the cloud home coordinates and the owner's signing key. The new device decrypts the snapshot and changesets from the cloud home, writes a local copy of the library database, and from then on reads and writes as the same owner. Every device is the one owner, full read/write -- there is no second person and no second identity.

The restore code is exchanged on your own (copy it across, read it from one screen to another, etc.). There is no directory and no account to look up.

## When things go wrong

The sync layer is designed to recover quietly, name the cause clearly, and tell you what to do.

- **Cloud storage is full.** The banner names the provider and where to free space — "Your Google Drive storage is full. Free up space at drive.google.com to keep syncing." (Same shape for Dropbox, OneDrive, S3.)
- **You've been signed out of the cloud account.** Refresh-token revoked, password changed, app permission pulled — bae shows "Your {provider} access was revoked or expired. Reconnect to keep syncing." Tapping the banner opens sync setup for re-OAuth.
- **S3 credentials lost permission, or the bucket's gone.** "Your S3 credentials don't have permission to write to this bucket. Check the access policy in sync settings." / "The S3 bucket no longer exists. Check the bucket name in sync settings."
- **Network's flaky.** Failures retry with exponential backoff (30s → 60 → 120 → 240, capped at 5 minutes); a successful sync resets the count. Manually triggering a sync from settings skips the wait.
- **Newer version of bae upgraded the library.** "N changes from a newer bae version were skipped. Update bae to apply them." A fresh snapshot reconciles the missed rows once you update.
- **Restore code didn't paste right.** The error names the likely cause — "doesn't look like a coven restore code" (missing `coven:` prefix), "incomplete or has a typo, check that you copied the entire code" (truncated paste), "the restore code is corrupted, regenerate it on the source device" (malformed payload), or "made with a newer version of bae" (version too high).

There's no "an error occurred." Sync errors are written for the human reading the banner.