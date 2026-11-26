package fm.bae.app.shots

import androidx.compose.ui.platform.LocalContext
import fm.bae.app.text
import fm.bae.app.ui.albumdetail.AndroidAlbumAudio
import org.junit.Test
import uniffi.bae_bridge.BridgeSourceAudioDifference
import uniffi.bae_bridge.BridgeSourceAudioSummary

/** The component shots of `AndroidAlbumAudio`, one for each row of its preview. */
class AndroidAlbumAudioShots : BaeBoardShot() {
    @Test
    fun androidComponentAlbumAudioAudioLine() =
        component("android.component.album-audio.audio-line") { AndroidAlbumAudio("FLAC · 44.1 kHz · 16-bit · Stereo") }

    @Test
    fun androidComponentAlbumAudioFilesDisagree() =
        component("android.component.album-audio.files-disagree") {
            val disagree =
                BridgeSourceAudioSummary.Mixed(
                    listOf(
                        BridgeSourceAudioDifference.Codec(listOf("FLAC", "MP3")),
                        BridgeSourceAudioDifference.SampleRate(listOf(44_100L, 48_000L)),
                    ),
                )
            AndroidAlbumAudio(disagree.text(LocalContext.current) { "core.audio.channels.stereo" })
        }
}
