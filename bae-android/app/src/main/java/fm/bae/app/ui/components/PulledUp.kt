package fm.bae.app.ui.components

import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Dp

/** Takes [by] off the height this takes in its parent, as a button row's own padding overhangs. */
internal fun Modifier.pulledUpBy(by: Dp): Modifier =
    layout { measurable, constraints ->
        val placeable = measurable.measure(constraints)
        layout(placeable.width, placeable.height - by.roundToPx()) { placeable.place(0, 0) }
    }
