package fm.bae.app.ui.components

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Dp

/**
 * Takes [top] and [bottom] off the height this takes in its parent, as a button row's own padding
 * overhangs: the content moves up by [top], and the parent leaves out that and [bottom].
 */
internal fun Modifier.pulledUpBy(
    top: Dp,
    bottom: Dp,
): Modifier =
    layout { measurable, constraints ->
        val placeable = measurable.measure(constraints)
        val height = (placeable.height - top.roundToPx() - bottom.roundToPx()).coerceAtLeast(0)
        layout(placeable.width, height) { placeable.place(0, -top.roundToPx()) }
    }
