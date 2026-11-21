package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AndroidUploadLine
import org.junit.Test

/** The component shots of `AndroidUploadLine`, one for each row of its preview. */
class AndroidUploadLineShots : BoardShot() {
    @Test
    fun androidComponentUploadLineQueued() =
        component("android.component.upload-line.queued") {
            AndroidUploadLine("Queued to upload", null)
        }

    @Test
    fun androidComponentUploadLineWaitingFor01HarborLightsFlac() =
        component("android.component.upload-line.waiting-for-01-harbor-lights-flac") {
            AndroidUploadLine("Queued to upload", "01 Harbor Lights.flac")
        }
}
