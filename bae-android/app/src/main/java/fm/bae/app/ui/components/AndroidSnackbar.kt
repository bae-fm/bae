package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarData
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R

/**
 * How much of the screen's bottom the now-playing bars take, for the snackbar to float above. Each bar that is
 * showing reports its height under its own key and takes it back when it leaves.
 */
class SnackbarLift {
    private val heights = mutableStateMapOf<Any, Int>()

    /** The tallest bar showing, in pixels; 0 when none is. */
    val height: Int get() = heights.values.maxOrNull() ?: 0

    internal fun report(
        key: Any,
        pixels: Int,
    ) {
        heights[key] = pixels
    }

    internal fun withdraw(key: Any) {
        heights.remove(key)
    }
}

/** The lift the app's snackbar host reads, if there is one. */
val LocalSnackbarLift = compositionLocalOf<SnackbarLift?> { null }

/** Marks this a bar the snackbar floats above: its measured height is the lift while it is composed. */
@Composable
fun Modifier.liftsSnackbar(): Modifier {
    val lift = LocalSnackbarLift.current ?: return this
    val key = remember { Any() }
    DisposableEffect(lift, key) { onDispose { lift.withdraw(key) } }
    return onSizeChanged { lift.report(key, it.height) }
}

/** The box the board gives the close glyph, and the glyph in it. */
private val CLOSE_BOX = 32.dp
private val CLOSE_GLYPH = 20.dp

/** The snackbar host over the whole screen, at its bottom. */
@Composable
fun AndroidSnackbarOverlay(
    state: SnackbarHostState,
    lift: SnackbarLift,
) {
    Box(modifier = Modifier.fillMaxSize()) {
        AndroidSnackbarHost(state = state, lift = lift, modifier = Modifier.align(Alignment.BottomCenter))
    }
}

/** The app's one snackbar host: [state]'s messages, floated above the now-playing bar when one is showing. */
@Composable
fun AndroidSnackbarHost(
    state: SnackbarHostState,
    lift: SnackbarLift,
    modifier: Modifier = Modifier,
) {
    val above = with(LocalDensity.current) { lift.height.toDp() }
    SnackbarHost(
        hostState = state,
        modifier = modifier.padding(bottom = above),
    ) { data -> AndroidSnackbar(data) }
}

/** A message in the inverse colours, with its action and its close, which is a 20 dp glyph in a 32 dp box. */
@Composable
private fun AndroidSnackbar(data: SnackbarData) {
    val visuals = data.visuals
    Snackbar(
        action =
            visuals.actionLabel?.let { label ->
                { TextButton(onClick = data::performAction) { Text(label) } }
            },
        dismissAction =
            if (visuals.withDismissAction) {
                {
                    IconButton(onClick = data::dismiss, modifier = Modifier.size(CLOSE_BOX)) {
                        Icon(
                            imageVector = Icons.Filled.Close,
                            contentDescription = stringResource(R.string.close),
                            modifier = Modifier.size(CLOSE_GLYPH),
                        )
                    }
                }
            } else {
                null
            },
    ) {
        Text(visuals.message)
    }
}
