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
import uniffi.bae_bridge.BridgeUploadWait
import uniffi.bae_bridge.bridgeUploadActivityKey
import java.text.DateFormat
import java.util.Date

/** The release's upload line over its download control. */
@Composable
internal fun ReleaseTransferLines(
    session: OpenLibrary,
    release: BridgeRelease,
) {
    Column {
        ReleaseUploadLine(release.upload?.progress, release.upload?.wait)
        ReleaseDownloadControl(session = session, release = release)
    }
}

/**
 * What of the release waits in this device's upload queue: the phase core
 * words ("Queued"), and, while none of its files is being sent, why they are
 * not moving. Nothing when no file of it is queued.
 */
@Composable
internal fun ReleaseUploadLine(
    progress: BridgeUploadProgress?,
    wait: BridgeUploadWait?,
) {
    if (progress == null) return
    UploadLineBody(progress.activity, wait)
}

@Composable
private fun UploadLineBody(
    activity: BridgeUploadActivity,
    wait: BridgeUploadWait?,
) {
    val words =
        buildList {
            add(LocalContext.current.coreString(bridgeUploadActivityKey(activity)))
            wait?.let { add(uploadWaitText(it)) }
        }
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Text(
            text = words.joinToString(" · "),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** The words for what keeps a release's waiting files from being sent. */
@Composable
private fun uploadWaitText(wait: BridgeUploadWait): String =
    when (wait) {
        is BridgeUploadWait.Original -> {
            stringResource(R.string.upload_waiting_for_file, wait.fileName)
        }

        BridgeUploadWait.Offline -> {
            stringResource(R.string.download_waiting_for_network)
        }

        BridgeUploadWait.Paused -> {
            stringResource(R.string.paused)
        }

        BridgeUploadWait.Metered -> {
            stringResource(R.string.upload_waiting_for_wifi)
        }

        is BridgeUploadWait.Retrying -> {
            stringResource(
                R.string.upload_retrying_at,
                DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(wait.atMs)),
            )
        }
    }

@Preview(showBackground = true)
@Composable
private fun UploadLinePausedPreview() {
    BaeTheme { UploadLineBody(BridgeUploadActivity.QUEUED, BridgeUploadWait.Paused) }
}

@Preview(showBackground = true)
@Composable
private fun UploadLineRetryingPreview() {
    BaeTheme { UploadLineBody(BridgeUploadActivity.QUEUED, BridgeUploadWait.Retrying(atMs = 1_800_000_000_000L)) }
}
