# AndroidFullPlayer

The full player, in a sheet over the library: Collapse, the cover, title, artists, seek bar, previous / play / next, repeat, volume. The upcoming queue continues under it.

`noVolume` (the receiver's name), as the Mac bar's: casting to a receiver that takes no volume from bae, the volume row is "Volume is set on Living Room" in place of mute and the slider.


**In the code:** Android: `AndroidFullPlayer` (`ui/playback/AndroidFullPlayer.kt`), taking the track and the seek row, transport, secondary controls and volume as slots; `AndroidFullPlayerContainer`, in the same file, reads the session and opens the sheet.
