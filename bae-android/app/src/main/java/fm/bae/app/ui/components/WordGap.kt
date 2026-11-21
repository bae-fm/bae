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

/** The characters in a group of a code. */
private const val CODE_GROUP = 4

/** Whether a code field draws [char]: the spaces, line breaks and dashes between groups are not drawn. */
private fun Char.isCodeGap(): Boolean = this == '-' || isWhitespace()

/**
 * A code field's text in groups of four with [WORD_GAP] after each space, whether it was typed in
 * groups or run together. The spaces and dashes of the typed text are not drawn, and the cursor keeps
 * its place among the characters that are.
 */
internal object CodeGroupTransformation : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val raw = text.text
        val kept = raw.indices.filter { !raw[it].isCodeGap() }
        val grouped = kept.chunked(CODE_GROUP).joinToString(" ") { group -> group.map { raw[it] }.joinToString("") }
        return TransformedText(grouped.withWordGap(), CodeGroupOffsets(kept, raw.length))
    }
}

/** Where the cursor goes between the typed text and its groups, given the typed text's [kept] character indices. */
private class CodeGroupOffsets(
    private val kept: List<Int>,
    private val rawLength: Int,
) : OffsetMapping {
    override fun originalToTransformed(offset: Int): Int {
        val before = kept.count { it < offset }
        return if (before == 0) 0 else before + (before - 1) / CODE_GROUP
    }

    override fun transformedToOriginal(offset: Int): Int {
        val before = offset - offset / (CODE_GROUP + 1)
        return if (before == 0) 0 else minOf(kept[minOf(before, kept.size) - 1] + 1, rawLength)
    }
}
