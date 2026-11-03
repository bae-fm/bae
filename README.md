# bae

bae is a music library and player for a household. It plays the music you
already have, as files on your own disks, and keeps one library on every
device in the household.

> [!WARNING]
> bae is pre-1.0 and under active development. Library and sync formats can
> change without migration.

## What it does

- **Imports folders of audio.** On a desktop, bae watches your music folders
  and looks each folder up on MusicBrainz and Discogs for its album, pressing,
  artists and cover. Folders of one file per track and disc images with a CUE
  sheet both import.
- **Leaves your files where they are.** bae reads and plays them; it never
  copies, moves or rewrites them.
- **Plays a release as it was made.** Tracks join without gaps, hidden pregaps
  play, and playback can pause between the sides of a record or the discs of a
  box set.
- **Shares the library.** Several people use one library, each with their own
  devices, ratings, listening history and playlists, kept private or shared.
- **Plays on other devices.** Phones browse, stream and pin releases for
  offline play; the apps cast to AirPlay, Chromecast and UPnP receivers; a
  desktop can serve the library to Subsonic apps.

## Sync

bae syncs through [coven](https://github.com/bae-fm/coven), using storage you
already have: a Dropbox or OneDrive folder, iCloud on Apple devices, or an
S3-compatible bucket. bae runs no server. Everything is encrypted on the device
before it reaches storage and kept under random names, so the provider sees no
album, artist or file names.

## Platforms

- **macOS:** the desktop app.
- **Android:** the phone app.
- **iOS and Windows:** the apps start, but have no library screens.
- **Linux:** no release.

bae also comes as **baeium**, which syncs only through S3-compatible storage,
sends no telemetry and needs nothing proprietary to build or run.

## More

[spec/bae.md](spec/bae.md) describes what bae does in detail. Downloads are at
[bae.fm](https://bae.fm).

## License

Unlicensed: all rights reserved while licensing is decided. See
[LICENSE](LICENSE).
