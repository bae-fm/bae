package fm.bae.app.shots

import androidx.compose.ui.unit.dp
import fm.bae.app.ui.albumdetail.AndroidTrackLead
import org.junit.Test

/** The component shots of `AndroidTrackLead`, one for each row of its preview. */
class AndroidTrackLeadShots : BoardShot() {
    @Test
    fun androidComponentTrackLeadNumber() =
        component("android.component.track-lead.number") {
            AndroidTrackLead(isCurrent = false, isPlaying = false, positionLabel = "4", width = 24.dp)
        }

    @Test
    fun androidComponentTrackLeadCurrent() =
        component("android.component.track-lead.current") {
            AndroidTrackLead(isCurrent = true, isPlaying = false, positionLabel = "4", width = 24.dp)
        }

    @Test
    fun androidComponentTrackLeadPlaying() =
        component("android.component.track-lead.playing") {
            AndroidTrackLead(isCurrent = true, isPlaying = true, positionLabel = "4", width = 24.dp)
        }
}
