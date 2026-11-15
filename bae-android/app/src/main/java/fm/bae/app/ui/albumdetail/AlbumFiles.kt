package fm.bae.app.ui.albumdetail

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import fm.bae.app.R
import fm.bae.app.fileKindText
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidEyebrow
import uniffi.bae_bridge.BridgeRelease

/**
 * The release's Files: the files no track plays from, each as its name at the start and what kind of
 * file it is at the end, under a hairline.
 */
internal fun LazyListScope.albumFiles(
    release: BridgeRelease,
    context: Context,
) {
    if (release.otherFiles.isEmpty()) return
    item {
        AndroidEyebrow(
            text = context.getString(R.string.album_files),
            modifier = Modifier.padding(bottom = ThemeSpace.inline),
            spaceAbove = ThemeSpace.inline,
        )
    }
    items(release.otherFiles, key = { it.id }) { file ->
        Column {
            HorizontalDivider()
            Row(
                modifier = Modifier.fillMaxWidth().padding(vertical = ThemeSpace.related),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(text = file.name, style = ThemeText.body.style)
                Text(
                    text = context.fileKindText(file.kind),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = ThemeSpace.group),
                )
            }
        }
    }
}
