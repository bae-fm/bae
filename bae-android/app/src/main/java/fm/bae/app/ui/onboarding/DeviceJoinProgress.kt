package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fm.bae.app.coreString
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import uniffi.bae_bridge.BridgeAdmittingDeviceJoinProgress
import uniffi.bae_bridge.BridgeJoiningDeviceJoinProgress
import uniffi.bae_bridge.bridgeAdmittingDeviceJoinProgressKey
import uniffi.bae_bridge.bridgeJoiningDeviceJoinProgressKey

@Composable
internal fun JoiningDeviceProgress(progress: BridgeJoiningDeviceJoinProgress) {
    val context = LocalContext.current
    val bytes =
        if (progress is BridgeJoiningDeviceJoinProgress.DownloadingSnapshot) {
            progress.bytesDone to progress.bytesTotal
        } else {
            null
        }
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        if (bytes != null && bytes.second > 0uL) {
            LinearProgressIndicator(
                progress = { bytes.first.toFloat() / bytes.second.toFloat() },
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            CircularProgressIndicator(modifier = Modifier.width(32.dp))
        }
        Spacer(modifier = Modifier.height(12.dp))
        Text(
            text = context.coreString(bridgeJoiningDeviceJoinProgressKey(progress)),
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
        )
        if (bytes != null) {
            Text(
                text =
                    "${context.formatFileSize(bytes.first.requireDisplayableByteCount())} / " +
                        context.formatFileSize(bytes.second.requireDisplayableByteCount()),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun AdmittingDeviceProgress(progress: BridgeAdmittingDeviceJoinProgress) {
    val context = LocalContext.current
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        CircularProgressIndicator(modifier = Modifier.width(32.dp))
        Spacer(modifier = Modifier.height(12.dp))
        Text(
            text = context.coreString(bridgeAdmittingDeviceJoinProgressKey(progress)),
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
        )
    }
}
