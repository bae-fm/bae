package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.albumdetail.AndroidTrackRow
import fm.bae.app.ui.albumdetail.TrackRowCallbacks
import fm.bae.app.ui.albumdetail.TrackRowData
import org.junit.Test
import uniffi.bae_bridge.BridgeTrackWork

/** The component shots of `AndroidTrackRow`, one for each row of its preview. */
class AndroidTrackRowShots : BoardShot() {
    @Test
    fun androidComponentTrackRowResting() =
        component("android.component.track-row.resting") {
            TrackRowShot(TrackRowData("1", "Harbor Lights", null, "4:32", isCurrent = false, isPlaying = false))
        }

    @Test
    fun androidComponentTrackRowCurrent() =
        component("android.component.track-row.current") {
            TrackRowShot(TrackRowData("2", "The Long Way Home", null, "5:12", isCurrent = true, isPlaying = false))
        }

    @Test
    fun androidComponentTrackRowPlaying() =
        component("android.component.track-row.playing") {
            TrackRowShot(TrackRowData("2", "The Long Way Home", null, "5:12", isCurrent = true, isPlaying = true))
        }

    @Test
    fun androidComponentTrackRowWork() =
        component("android.component.track-row.work") {
            Column {
                AndroidTrackRow(
                    TrackRowData("4", "Low Tide Hours", null, "5:03", isCurrent = false, isPlaying = false),
                    callbacks,
                    positionColumnWidth = 40.dp,
                    works = listOf(BridgeTrackWork("work-ebb", "Low Tide Suite: I. Ebb", "Ada Lindqvist")),
                )
            }
        }

    @Test
    fun androidComponentTrackRowCompilation() =
        component("android.component.track-row.compilation") {
            TrackRowShot(TrackRowData("3", "Low Tide Hours", "Mara Quill", "5:03", isCurrent = false, isPlaying = false))
        }
}

private val callbacks = TrackRowCallbacks(onClick = {}, onPlayNext = {}, onAddToQueue = {})

@Composable
private fun TrackRowShot(data: TrackRowData) = AndroidTrackRow(data, callbacks, positionColumnWidth = 40.dp)
