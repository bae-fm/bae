# MacVolumeSlider

Mute and the volume slider at the end of the now playing bar.

| Prop | What it does |
|---|---|
| `volume` | How full the slider is, from 0 to 1 (0.7 unless given). |
| `noVolume` | bae casts to a receiver that takes no volume from it, such as a UPnP renderer with no volume control: no Mute and no slider, and a line naming the receiver, where the person sets the volume. |

**In the code:** macOS: `SlimSlider` (`Views/Playback/NowPlaying/SlimSlider.swift`).
