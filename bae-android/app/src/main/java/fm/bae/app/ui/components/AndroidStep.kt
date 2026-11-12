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
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * One step of joining a library, in its `top` form: a screen to fill in, which
 * sits at the top, left-aligned as a form reads, with its button right under
 * its fields. A [title] that says what to do or what happened, an optional
 * [text] line under it in secondary text, then the step's own [content], a
 * section apart. The title wraps balanced, so it never leaves one word alone
 * on its last line, in any language.
 */
@Composable
internal fun AndroidStep(
    title: String,
    modifier: Modifier = Modifier,
    text: String? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
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
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.section),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
            Text(
                text = title,
                style = ThemeText.title.style.copy(lineBreak = LineBreak.Heading),
                textAlign = TextAlign.Start,
            )
            text?.let {
                Text(
                    text = it,
                    style = ThemeText.body.style.copy(lineBreak = LineBreak.Paragraph),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Start,
                )
            }
        }
        content()
    }
}
