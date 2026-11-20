package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.vectorResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidGalleryCaption
import fm.bae.app.ui.albumdetail.AndroidReleaseFiles
import fm.bae.app.ui.albumdetail.AndroidTrackLead
import fm.bae.app.ui.albumdetail.AndroidUnplayableLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDivider
import fm.bae.app.ui.components.AndroidErrorLine
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidListRow
import fm.bae.app.ui.components.AndroidSegmentedControl
import fm.bae.app.ui.components.AndroidUsageKey
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.IconTone
import fm.bae.app.ui.components.StorageUsageSegment
import fm.bae.app.ui.components.TrackOptionsMenu
import fm.bae.app.ui.downloads.AndroidPinsAtOnce
import fm.bae.app.ui.playback.AndroidPlaceholderLine
import org.junit.Test
import uniffi.bae_bridge.BridgeFileKind
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeStorageKind

/** The component shots of the small pieces: divider, error line, usage key, track lead, placeholder line, list row, segmented control, pins at once, release files and gallery caption. */
class ElementComponentShots : BoardShot() {
    @Test
    fun androidComponentDividerDivider() =
        component("android.component.divider.divider", wide = true) {
            Column {
                Text("Above", modifier = Modifier.fillMaxWidth().padding(vertical = ThemeSpace.related))
                AndroidDivider()
                Text("Below", modifier = Modifier.fillMaxWidth().padding(vertical = ThemeSpace.related))
            }
        }

    @Test
    fun androidComponentErrorLineError() =
        component("android.component.error-line.error", wide = true) { AndroidErrorLine("bae couldn’t read the queue.") }

    @Test
    fun androidComponentErrorLineLongError() =
        component("android.component.error-line.long-error", wide = true) {
            AndroidErrorLine("bae couldn’t read the queue. The library file on this phone is damaged; open the library again to repair it.")
        }

    @Test
    fun androidComponentUsageKeyPinned() =
        component("android.component.usage-key.pinned") { AndroidUsageKey(key(BridgeStorageKind.PINNED, "Pinned", "6.1 GB")) }

    @Test
    fun androidComponentUsageKeyCache() =
        component("android.component.usage-key.cache") { AndroidUsageKey(key(BridgeStorageKind.CACHE, "Cache", "1.8 GB")) }

    @Test
    fun androidComponentUsageKeyLibrary() =
        component("android.component.usage-key.library") { AndroidUsageKey(key(BridgeStorageKind.LIBRARY, "Library", "420 MB")) }

    @Test
    fun androidComponentUsageKeyFromBeforeTheReset() =
        component("android.component.usage-key.from-before-the-reset") {
            AndroidUsageKey(key(BridgeStorageKind.BEFORE_RESET, "From before the reset", "960 MB"))
        }

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

    @Test
    fun androidComponentPlaceholderLineLine() =
        component("android.component.placeholder-line.line") { AndroidPlaceholderLine(width = 160.dp, height = 12.dp) }

    @Test
    fun androidComponentPlaceholderLineShortLine() =
        component("android.component.placeholder-line.short-line") { AndroidPlaceholderLine(width = 100.dp, height = 10.dp) }

    @Test
    fun androidComponentUnplayableLineOnAnotherDevice() =
        component("android.component.unplayable-line.on-another-device", wide = true) { Elsewhere("Ana’s Mac") }

    @Test
    fun androidComponentUnplayableLineOnAPhone() =
        component("android.component.unplayable-line.on-a-phone", wide = true) { Elsewhere("Ben’s phone") }

    @Test
    fun androidComponentListRowResting() = component("android.component.list-row.resting", wide = true) { Entry() }

    @Test
    fun androidComponentListRowMenu() = component("android.component.list-row.menu", wide = true) { Entry(menu = true) }

    @Test
    fun androidComponentListRowDragging() =
        component("android.component.list-row.dragging", wide = true) {
            Entry(menu = true, dragging = true)
        }

    @Test
    fun androidComponentSegmentedControlChoice() =
        component("android.component.segmented-control.choice", wide = true) {
            AndroidSegmentedControl(options = (1..6).map { "$it" }, selected = 2, onSelect = {})
        }

    @Test
    fun androidComponentSegmentedControlWithALabel() =
        component("android.component.segmented-control.with-a-label", wide = true) {
            AndroidSegmentedControl(options = listOf("Member", "Owner"), selected = 0, onSelect = {}, label = "Role")
        }

    @Test
    fun androidComponentSegmentedControlWithAHelper() =
        component("android.component.segmented-control.with-a-helper", wide = true) {
            AndroidSegmentedControl(
                options = listOf("Only me", "Everyone"),
                selected = 1,
                onSelect = {},
                label = "Who sees it",
                helper = "Everyone in the library sees this playlist and can play it. Only you can change it.",
            )
        }

    @Test
    fun androidComponentPinsAtOnceThreeAtOnce() =
        component("android.component.pins-at-once.three-at-once", wide = true) { AndroidPinsAtOnce(value = 3u, limit = 6u, onSelect = {}) }

    @Test
    fun androidComponentPinsAtOnceOneOfFour() =
        component("android.component.pins-at-once.one-of-four", wide = true) { AndroidPinsAtOnce(value = 1u, limit = 4u, onSelect = {}) }

    @Test
    fun androidComponentReleaseFilesThreeFiles() =
        component("android.component.release-files.three-files", wide = true) {
            AndroidReleaseFiles(
                listOf(
                    boardFile("Low Tide Hours.cue", BridgeFileKind.CueSheet),
                    boardFile("Low Tide Hours.log", BridgeFileKind.RipLog),
                    boardFile("booklet.pdf", BridgeFileKind.Pdf),
                ),
            )
        }

    @Test
    fun androidComponentReleaseFilesOneFile() =
        component("android.component.release-files.one-file", wide = true) {
            AndroidReleaseFiles(listOf(boardFile("Low Tide Hours.log", BridgeFileKind.RipLog)))
        }

    @Test
    fun androidComponentReleaseFilesLongName() =
        component("android.component.release-files.long-name", wide = true) {
            AndroidReleaseFiles(
                listOf(boardFile("Songs for the Slow Orchards, Recorded Live at the Harbor Hall.cue", BridgeFileKind.CueSheet)),
            )
        }

    @Test
    fun androidComponentGalleryCaptionSeveralImages() =
        component("android.component.gallery-caption.several-images", wide = true) {
            OnTheBackdrop { AndroidGalleryCaption(label = "cover.jpg", page = 1, count = 4u) }
        }

    @Test
    fun androidComponentGalleryCaptionOneImage() =
        component("android.component.gallery-caption.one-image", wide = true) {
            OnTheBackdrop { AndroidGalleryCaption(label = "cover.jpg", page = 1, count = 1u) }
        }

    @Test
    fun androidComponentButtonWithIcon() =
        component("android.component.button.with-icon") {
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = {}, icon = ImageVector.vectorResource(R.drawable.ic_keep)) { Text("Pin") }
        }

    @Test
    fun androidComponentIconButtonOnTheBackdrop() =
        component("android.component.icon-button.on-the-backdrop") {
            OnTheBackdrop { AndroidIconButton(Icons.Filled.Close, "Close", onClick = {}, tone = IconTone.ON_FILL) }
        }
}

@Composable
private fun OnTheBackdrop(content: @Composable () -> Unit) = Row(modifier = Modifier.background(BaeTheme.colors.backdrop)) { content() }

@Composable
private fun Elsewhere(device: String) =
    AndroidUnplayableLine(BridgeMessage("core.release.on_device", listOf(BridgeMessageArg("device", device))))

@Composable
private fun Entry(
    menu: Boolean = false,
    dragging: Boolean = false,
) = Images {
    AndroidListRow(
        title = "Harbor Lights",
        subtitle = "Mara Quill · Low Tide Hours",
        trailing = "4:32",
        art = PreviewData.imageRef(),
        onClick = {},
        dragging = dragging,
        menu = if (menu) ({ TrackOptionsMenu(onPlayNext = {}, onAddToQueue = {}) }) else null,
    )
}

private fun key(
    kind: BridgeStorageKind,
    label: String,
    size: String,
) = StorageUsageSegment(kind, label, size, 0f)
