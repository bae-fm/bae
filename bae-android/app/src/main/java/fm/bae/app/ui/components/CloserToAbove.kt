package fm.bae.app.ui.components

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Dp

/** Draws this [by] closer to what is above it, and takes [by] less room, so what follows keeps its gap. */
internal fun Modifier.closerToAbove(by: Dp): Modifier =
    layout { measurable, constraints ->
        val placeable = measurable.measure(constraints)
        val pull = by.roundToPx()
        layout(placeable.width, placeable.height - pull) { placeable.place(0, -pull) }
    }
