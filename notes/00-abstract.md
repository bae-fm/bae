# Abstract

bae is a personal music library manager that uses a locally generated identity and end-to-end encryption over storage you control (S3, Google Drive, Dropbox, OneDrive, or iCloud) to sync one library across your own devices.

- **Identity**: one locally generated keypair per device (Ed25519/X25519), shared across your devices by the restore code. The public key is the owner identity. There is no central identity server and no account system; bae runs no server of its own.
- **Encryption**: one symmetric key per library. Everything in the cloud home is encrypted. The storage provider sees opaque blobs.
- **Storage**: commercial cloud providers, any S3-compatible bucket, or local-only.
- **Sync**: your devices sync one library through the cloud home via encrypted, signed changesets. A second device connects with a restore code that carries the owner's signing key, so every device is the one owner with full read/write access.
