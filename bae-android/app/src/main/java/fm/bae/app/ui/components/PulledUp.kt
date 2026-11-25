package fm.bae.app.ui.components

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp

/**
 * Takes [top] and [bottom] off the height this takes in its parent, as a button row's own padding
 * overhangs: the content moves up by [top], and the parent leaves out that and [bottom]. The content is
 * measured as tall as it wants, since it overhangs the room its parent has left for it.
 */
internal fun Modifier.pulledUpBy(
    top: Dp,
    bottom: Dp,
): Modifier =
    layout { measurable, constraints ->
        val placeable = measurable.measure(constraints.copy(maxHeight = Constraints.Infinity))
        val height = (placeable.height - top.roundToPx() - bottom.roundToPx()).coerceAtLeast(0)
        layout(placeable.width, height) { placeable.place(0, -top.roundToPx()) }
    }

/**
 * Takes [end] off the width this takes in its parent, as a button row's own padding overhangs the notice's edge:
 * the content keeps its place, and the parent leaves the words the room that the overhang would have taken.
 */
internal fun Modifier.overhangingEnd(end: Dp): Modifier =
    layout { measurable, constraints ->
        val placeable = measurable.measure(constraints.copy(maxWidth = Constraints.Infinity))
        layout((placeable.width - end.roundToPx()).coerceAtLeast(0), placeable.height) { placeable.place(0, 0) }
    }
