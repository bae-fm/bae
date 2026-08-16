package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.pauseRequested
import fm.bae.app.performBridgeAction
import kotlinx.coroutines.launch

private val logger = BaeLogger("bae.SyncUploadPauseControl")

@Composable
internal fun SyncUploadPauseControl(session: OpenLibrary) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val outbox by session.outboxStore.snapshot.collectAsState()

    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(R.string.settings_pause_uploads), modifier = Modifier.weight(1f))
        Switch(
            checked = outbox.pauseRequested,
            onCheckedChange = { paused ->
                scope.launch {
                    performBridgeAction(
                        logger = logger,
                        operation = "set sync pause state",
                        errors = LocaleErrorLines(context),
                        showError = session.configStore::showError,
                    ) {
                        session.appHandle.setSyncPaused(paused)
                    }
                }
            },
        )
    }
    Text(
        text = stringResource(R.string.settings_pause_uploads_footer),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
