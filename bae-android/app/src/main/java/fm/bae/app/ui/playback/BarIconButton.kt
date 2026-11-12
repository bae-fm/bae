package fm.bae.app.ui.playback

import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.unit.Dp

/**
 * One of the bar's icon controls: a 40 dp box, as the design draws them, where Material's
 * [IconButton] lays out 48 dp around its 40 dp container. Compose still takes a press up to
 * 48 dp from the box's centre (`ViewConfiguration.minimumTouchTargetSize`), so the touch
 * target stays Android's minimum.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun BarIconButton(
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
        IconButton(onClick = onClick, content = content)
    }
}
