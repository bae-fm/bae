package fm.bae.app.ui.sync

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CloudDownload
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** "Arriving from <device>", the quiet line for files another device is still uploading. */
@Composable
internal fun arrivingFromText(deviceName: String): String =
    LocalContext.current.coreString("core.arrival.from", mapOf("device" to deviceName))

/** A small cloud mark on an album whose files are still arriving from another device. */
@Composable
internal fun AlbumArrivalBadge(
    deviceName: String,
    modifier: Modifier = Modifier,
) {
    Icon(
        imageVector = Icons.Filled.CloudDownload,
        contentDescription = arrivingFromText(deviceName),
        tint = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier =
            modifier
                .background(MaterialTheme.colorScheme.surface, CircleShape)
                .padding(ThemeSpace.inline)
                .size(14.dp),
    )
}

/** The calm line at the top of a release's detail while some of its files are still arriving. */
@Composable
internal fun ReleaseArrivalLine(deviceName: String) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = Icons.Filled.CloudDownload,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(14.dp),
        )
        Text(
            text = arrivingFromText(deviceName),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
