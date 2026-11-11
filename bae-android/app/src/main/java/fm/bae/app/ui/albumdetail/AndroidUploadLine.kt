package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.StatusTone
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeUploadProgress
import uniffi.bae_bridge.bridgeUploadActivityKey

/** The release's upload line over its download control. */
@Composable
internal fun ReleaseTransferLines(
    session: OpenLibrary,
    release: BridgeRelease,
) {
    Column {
        AndroidUploadLineContainer(release.upload?.progress, release.upload?.missing)
        AndroidDownloadControlContainer(session = session, release = release)
    }
}

/**
 * What of the release waits in this device's upload queue: the phase core
 * words ("Queued to upload") and, while none of its files is being sent, a
 * quiet warning naming the file whose missing original its move waits for.
 * Nothing when no file of it is queued.
 */
@Composable
internal fun AndroidUploadLineContainer(
    progress: BridgeUploadProgress?,
    missing: String?,
) {
    if (progress == null) return
    AndroidUploadLine(LocalContext.current.coreString(bridgeUploadActivityKey(progress.activity)), missing)
}

/** The upload line from its [phase] words, so a preview draws it without a call into the bridge. */
@Composable
internal fun AndroidUploadLine(
    phase: String,
    missing: String?,
) {
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
        Text(
            text = phase,
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (missing != null) {
            AndroidNotice(
                title = stringResource(R.string.upload_missing_title, missing),
                advice = stringResource(R.string.upload_missing_advice),
                error = null,
                tone = StatusTone.WARNING,
                look = NoticeLook.QUIET,
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun UploadLineQueuedPreview() {
    BaeTheme { AndroidUploadLine(LocalContext.current.coreString("core.outbox.phase.queued"), null) }
}

@Preview(showBackground = true)
@Composable
private fun UploadLineMissingPreview() {
    BaeTheme { AndroidUploadLine(LocalContext.current.coreString("core.outbox.phase.queued"), "01 Track.flac") }
}
