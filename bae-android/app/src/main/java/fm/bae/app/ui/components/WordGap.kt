package fm.bae.app.ui.components

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.sp

/** The extra room the design leaves after each space of a code, so its groups read apart. */
private val WORD_GAP = 4.sp

/** This text with [WORD_GAP] after each space: Compose has no word spacing, so the spaces carry letter spacing. */
internal fun String.withWordGap(): AnnotatedString =
    buildAnnotatedString {
        append(this@withWordGap)
        this@withWordGap.forEachIndexed { index, char ->
            if (char == ' ') addStyle(SpanStyle(letterSpacing = WORD_GAP), index, index + 1)
        }
    }

/** A code field's text with [WORD_GAP] after each space; every character keeps its place. */
internal object WordGapTransformation : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val spaced = text.text.withWordGap()
        return TransformedText(spaced, OffsetMapping.Identity)
    }
}
