package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.fileKindText
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidDivider
import fm.bae.app.ui.components.AndroidEyebrow
import uniffi.bae_bridge.BridgeFile

/**
 * The release's Files: the [files] no track plays from, under the Files eyebrow, each as its name at the start and
 * what kind of file it is at the end, under a hairline.
 */
@Composable
internal fun AndroidReleaseFiles(
    files: List<BridgeFile>,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    Column(modifier = modifier) {
        AndroidEyebrow(
            text = stringResource(R.string.album_files),
            modifier = Modifier.padding(bottom = ThemeSpace.inline),
            spaceAbove = ThemeSpace.inline,
        )
        files.forEach { file ->
            AndroidDivider()
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
