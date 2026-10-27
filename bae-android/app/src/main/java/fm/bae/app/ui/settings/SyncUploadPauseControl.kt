package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure

@Composable
internal fun SyncUploadPauseControl(session: OpenLibrary) {
    val uploadsPaused by session.outboxStore.uploadsPaused.collectAsState()
    val paused =
        when (val current = uploadsPaused) {
            is LiveValue.Current -> {
                current.value
            }

            // Whether uploads are paused is unknown while its read has failed.
            is LiveValue.Failed -> {
                Text(stringResource(R.string.settings_pause_uploads))
                ErrorDetailDisclosure(current.error)
                return
            }
        }

    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(R.string.settings_pause_uploads), modifier = Modifier.weight(1f))
        Switch(
            checked = paused,
            onCheckedChange = { paused ->
                session.messages.perform("set sync pause state") {
                    session.appHandle.setSyncPaused(paused)
                }
            },
        )
    }
    Text(
        text = stringResource(R.string.settings_pause_uploads_footer),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
