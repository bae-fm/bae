package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeUploadActivity
import uniffi.bae_bridge.BridgeUploadProgress
import uniffi.bae_bridge.bridgeUploadActivityKey

/** The release's upload line over its download control. */
@Composable
internal fun ReleaseTransferLines(
    session: OpenLibrary,
    release: BridgeRelease,
) {
    Column {
        ReleaseUploadLine(release.upload?.progress, release.upload?.missing)
        ReleaseDownloadControl(session = session, release = release)
    }
}

/**
 * What of the release waits in this device's upload queue: the phase core
 * words ("Queued to upload"), and, while none of its files is being sent, the
 * file whose missing original its move waits for. Nothing when no file of it
 * is queued.
 */
@Composable
internal fun ReleaseUploadLine(
    progress: BridgeUploadProgress?,
    missing: String?,
) {
    if (progress == null) return
    UploadLineBody(progress.activity, missing)
}

@Composable
private fun UploadLineBody(
    activity: BridgeUploadActivity,
    missing: String?,
) {
    val words =
        buildList {
            add(LocalContext.current.coreString(bridgeUploadActivityKey(activity)))
            missing?.let { add(stringResource(R.string.upload_waiting_for_file, it)) }
        }
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Text(
            text = words.joinToString(" · "),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun UploadLineQueuedPreview() {
    BaeTheme { UploadLineBody(BridgeUploadActivity.QUEUED, null) }
}

@Preview(showBackground = true)
@Composable
private fun UploadLineMissingPreview() {
    BaeTheme { UploadLineBody(BridgeUploadActivity.QUEUED, "01 Track.flac") }
}
