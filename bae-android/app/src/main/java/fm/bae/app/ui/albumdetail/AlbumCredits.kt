package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.R
import fm.bae.app.creditLineText
import fm.bae.app.creditRoleText
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidEyebrow
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeRelease

/**
 * The release's Credits: who did what on it and its tracks, a row for each role and set of tracks core
 * groups, the role at the start and the artists at the end. A phone's page names no tracks.
 */
internal fun LazyListScope.albumCredits(release: BridgeRelease) {
    if (release.credits.isEmpty()) return
    item { AndroidAlbumCredits(release.credits) }
}

/** The Credits heading, then a row for each of the [credits]: the role at the start, the artists at the end. */
@Composable
internal fun AndroidAlbumCredits(credits: List<BridgeCreditRow>) {
    val context = LocalContext.current
    Column {
        AndroidEyebrow(
            text = stringResource(R.string.search_section_credits),
            modifier = Modifier.padding(bottom = ThemeSpace.inline),
            // The board spaces a block 8dp from the one above, and its own heading pads 4dp over the eyebrow.
            spaceAbove = ThemeSpace.inline + ThemeSpace.related,
        )
        credits.forEach { row ->
            Row(
                modifier = Modifier.fillMaxWidth().padding(vertical = ThemeSpace.compact),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    text = context.creditRoleText(row.role),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Text(
                    text = context.creditLineText(row.artists),
                    style = ThemeText.body.style,
                    textAlign = TextAlign.End,
                    modifier = Modifier.weight(1f).padding(start = ThemeSpace.group),
                )
            }
        }
    }
}
