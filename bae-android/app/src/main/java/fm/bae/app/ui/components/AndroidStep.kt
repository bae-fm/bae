package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * One step of joining a library. A [title] that says what to do or what
 * happened, an optional [text] line under it in secondary text, then the
 * step's own [content], a section apart. The title wraps balanced, so it
 * never leaves one word alone on its last line, in any language.
 *
 * A step to read or fill in is [top]: it sits at the top, left-aligned as a
 * form reads, with its button right under its fields. A step whose content is
 * one thing to look at, a code, is centred: its text is centred and it fills
 * the space it is given, so give it the screen's remaining height.
 */
@Composable
internal fun AndroidStep(
    title: String,
    modifier: Modifier = Modifier,
    text: String? = null,
    top: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    val align = if (top) TextAlign.Start else TextAlign.Center
    Column(
        modifier =
            modifier.fillMaxWidth().padding(
                PaddingValues(
                    start = ThemeSpace.edge,
                    end = ThemeSpace.edge,
                    top = ThemeSpace.section,
                    bottom = ThemeSpace.page,
                ),
            ),
        verticalArrangement =
            Arrangement.spacedBy(ThemeSpace.section, if (top) Alignment.Top else Alignment.CenterVertically),
        horizontalAlignment = if (top) Alignment.Start else Alignment.CenterHorizontally,
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            horizontalAlignment = if (top) Alignment.Start else Alignment.CenterHorizontally,
        ) {
            Text(
                text = title,
                style = ThemeText.title.style.copy(fontWeight = FontWeight.Medium, lineBreak = LineBreak.Heading),
                textAlign = align,
            )
            text?.let {
                Text(
                    text = it,
                    style = ThemeText.body.style.copy(lineBreak = LineBreak.Paragraph),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = align,
                )
            }
        }
        content()
    }
}
