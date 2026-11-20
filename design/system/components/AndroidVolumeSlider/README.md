# AndroidVolumeSlider

The full player's volume bar, drawn as its seek line is: the level (`value`, 0 to 1, 0.7 when unset) filled in the accent, a gap, then the rest in the accent's tint ending in a dot, with the handle at the level. `AndroidFullPlayer` draws it after the mute button.

**In the code:** Android: `AndroidVolumeSlider` (`ui/playback/AndroidVolumeSlider.kt`), in the full player (`ui/playback/AndroidFullPlayer.kt`).
