package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.layout
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * Groups related [CardRow]s on a screen: one surface with rounded corners,
 * dividing the rows it holds by hairlines. A child that isn't a [CardRow] takes
 * [cardRowDivider] to join the division.
 */
@Composable
internal fun RowCard(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier =
            modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(ThemeRadius.card))
                .background(MaterialTheme.colorScheme.surfaceContainer)
                // Each row draws a hairline on its top edge; the first row's
                // sits one hairline above the card, where the clip cuts it. A card
                // with no rows has no hairline to cut and measures 0 high.
                .layout { measurable, constraints ->
                    val hairline = HAIRLINE.roundToPx()
                    val placeable = measurable.measure(constraints)
                    val height = (placeable.height - hairline).coerceAtLeast(0)
                    layout(placeable.width, height) { placeable.place(0, -hairline) }
                },
    ) {
        CompositionLocalProvider(LocalInRowCard provides true) { content() }
    }
}

private val LocalInRowCard = staticCompositionLocalOf { false }

/** The hairline on a row's top edge, between it and the row above it in a [RowCard]; none outside one. */
@Composable
internal fun Modifier.cardRowDivider(): Modifier {
    if (!LocalInRowCard.current) return this
    val color = MaterialTheme.colorScheme.outlineVariant
    return drawBehind { drawLine(color, Offset(0f, 0f), Offset(size.width, 0f), strokeWidth = HAIRLINE.toPx()) }
}

private val HAIRLINE = 1.dp

@Preview(showBackground = true)
@Composable
private fun RowCardPreview() {
    BaeTheme {
        RowCard(modifier = Modifier.padding(ThemeSpace.edge)) {
            CardRow(title = "Status", subtitle = "Up to date", tone = StatusTone.SUCCESS)
            CardRow(title = "Library Name", subtitle = "Dropbox · name@example.com", selected = true, onClick = {})
            CardRow(title = "Add device", action = true, leadingIcon = Icons.Filled.Add, onClick = {})
        }
    }
}
