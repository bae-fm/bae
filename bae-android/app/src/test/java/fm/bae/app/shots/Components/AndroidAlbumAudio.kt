package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AndroidAlbumAudio
import org.junit.Test

/** The component shots of `AndroidAlbumAudio`, one for each row of its preview. */
class AndroidAlbumAudioShots : BoardShot() {
    @Test
    fun androidComponentAlbumAudioAudioLine() =
        component("android.component.album-audio.audio-line") { AndroidAlbumAudio("FLAC · 44.1 kHz · 16-bit · Stereo") }

    @Test
    fun androidComponentAlbumAudioFilesDisagree() =
        component("android.component.album-audio.files-disagree") {
            AndroidAlbumAudio("FLAC and MP3 · 44.1 and 48 kHz · 16-bit · Stereo")
        }
}
