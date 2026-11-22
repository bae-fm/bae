package fm.bae.app.ui.playback

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace

/** A skeleton for a row whose item hasn't loaded yet. */
@Composable
internal fun AndroidQueuePlaceholderRow() {
    val missingArt = BaeTheme.surfaces.missingArt
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier =
                Modifier
                    .size(ThemeSize.rowArtwork)
                    .clip(RoundedCornerShape(ThemeRadius.artwork))
                    .background(missingArt),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            AndroidPlaceholderLine(width = 160.dp, height = 12.dp)
            Spacer(modifier = Modifier.height(ThemeSpace.compact))
            AndroidPlaceholderLine(width = 100.dp, height = 10.dp)
            Spacer(modifier = Modifier.height(ThemeSpace.compact))
            AndroidPlaceholderLine(width = 120.dp, height = 10.dp)
        }
    }
}

/** A grey bar of [width] and [height] where a line of text will be. */
@Composable
internal fun AndroidPlaceholderLine(
    width: Dp,
    height: Dp,
) {
    Box(
        modifier =
            Modifier
                .size(width = width, height = height)
                .clip(RoundedCornerShape(ThemeRadius.bar))
                .background(BaeTheme.surfaces.missingArt),
    )
}
