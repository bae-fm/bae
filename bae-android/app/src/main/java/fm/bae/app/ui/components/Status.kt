package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** What a chip or notice says about its subject, and the colour that says it. */
enum class StatusTone { NEUTRAL, ACCENT, INFO, SUCCESS, WARNING, DANGER, ACTIVITY }

/** The text, glyph and outline colour. */
val StatusTone.color: Color
    @Composable
    get() =
        when (this) {
            StatusTone.NEUTRAL -> MaterialTheme.colorScheme.onSurfaceVariant
            StatusTone.ACCENT -> MaterialTheme.colorScheme.primary
            StatusTone.INFO -> BaeTheme.colors.info
            StatusTone.SUCCESS -> BaeTheme.colors.success
            StatusTone.WARNING -> BaeTheme.colors.warning
            StatusTone.DANGER -> BaeTheme.colors.danger
            StatusTone.ACTIVITY -> BaeTheme.colors.activity
        }

/** The fill behind the tone's own text. */
val StatusTone.fill: Color
    @Composable
    get() = color.copy(alpha = ThemeOpacity.tint)

/**
 * The fill of a filled notice: a toned notice has its own, a pale tone in light mode and the neutral
 * fill in dark; the rest keep [fill].
 */
private val StatusTone.noticeFill: Color
    @Composable
    get() =
        when (this) {
            StatusTone.INFO -> BaeTheme.colors.infoNotice
            StatusTone.SUCCESS -> BaeTheme.colors.successNotice
            StatusTone.WARNING -> BaeTheme.colors.warningNotice
            StatusTone.DANGER -> BaeTheme.colors.dangerNotice
            StatusTone.NEUTRAL, StatusTone.ACCENT, StatusTone.ACTIVITY -> fill
        }

/** Whether a filled notice in this tone has its own fill, under `onNotice` text: the tones with a glyph. */
private val StatusTone.hasNoticeFill: Boolean
    get() = icon(outline = false) != null

/** Whether the content sits on a filled notice's own fill. */
private val LocalOnNoticeFill = compositionLocalOf { false }

/**
 * The colour of a button in a notice. On a filled toned notice's own fill: the accent. Anywhere else,
 * the screen's text.
 */
internal val noticeButtonColor: Color
    @Composable
    get() =
        if (LocalOnNoticeFill.current) MaterialTheme.colorScheme.primary else LocalContentColor.current

/** The glyph a notice in this tone leads with: a filled circle with its mark, or its outline when [outline]. */
private fun StatusTone.icon(outline: Boolean): ImageVector? =
    when (this) {
        StatusTone.INFO -> if (outline) Icons.Outlined.Info else Icons.Filled.Info
        StatusTone.SUCCESS -> if (outline) Icons.Outlined.CheckCircle else Icons.Filled.CheckCircle
        StatusTone.WARNING, StatusTone.DANGER -> if (outline) Icons.Outlined.ErrorOutline else Icons.Filled.Error
        StatusTone.NEUTRAL, StatusTone.ACCENT, StatusTone.ACTIVITY -> null
    }

/** The glyph's colour: the tone's status icon colour; the rest have no glyph and use [color]. */
private val StatusTone.iconColor: Color
    @Composable
    get() =
        when (this) {
            StatusTone.INFO -> BaeTheme.colors.infoIcon
            StatusTone.SUCCESS -> BaeTheme.colors.successIcon
            StatusTone.WARNING -> BaeTheme.colors.warningIcon
            StatusTone.DANGER -> BaeTheme.colors.dangerIcon
            StatusTone.NEUTRAL, StatusTone.ACCENT, StatusTone.ACTIVITY -> color
        }

/**
 * The tone's glyph in the tone's glyph colour, or the secondary text colour when [muted]; nothing for
 * a tone without one.
 */
@Composable
fun StatusGlyph(
    tone: StatusTone,
    modifier: Modifier = Modifier,
    size: Dp = ThemeIcon.medium,
    outline: Boolean = false,
    muted: Boolean = false,
) {
    tone.icon(outline)?.let { icon ->
        Icon(
            icon,
            contentDescription = null,
            tint = glyphTint(tone, muted),
            modifier = modifier.size(size),
        )
    }
}

@Composable
private fun glyphTint(
    tone: StatusTone,
    muted: Boolean,
): Color =
    when {
        muted -> MaterialTheme.colorScheme.onSurfaceVariant
        else -> tone.iconColor
    }

/** A short label on a tinted fill: a status, a count, a tag or a role. */
@Composable
fun StatusChip(
    text: String,
    modifier: Modifier = Modifier,
    tone: StatusTone = StatusTone.NEUTRAL,
) {
    Text(
        text = text,
        style = ThemeText.chip.style,
        color = tone.color,
        maxLines = 1,
        modifier =
            modifier
                .background(tone.fill, RoundedCornerShape(ThemeRadius.chip))
                .padding(horizontal = ThemeSpace.compact, vertical = ThemeSpace.line),
    )
}

/**
 * How a notice sits on its screen: on its tone's fill; with no fill and an outline glyph; or, for a
 * view that failed whole, centred in the space it's given (drawn by [AndroidNotice], not by [NoticeFrame]).
 */
enum class NoticeLook { FILLED, QUIET, CENTERED }

/**
 * A notice in its tone: the tone's glyph at the top left, centred on the first
 * line of body text beside it. [NoticeLook.FILLED] draws it on the tone's fill;
 * [NoticeLook.QUIET] on the screen, with an outline glyph and no side padding.
 */
@Composable
internal fun NoticeFrame(
    tone: StatusTone,
    modifier: Modifier = Modifier,
    look: NoticeLook = NoticeLook.FILLED,
    content: @Composable RowScope.() -> Unit,
) {
    val quiet = look != NoticeLook.FILLED
    val onFill = !quiet && tone.hasNoticeFill
    val shape = RoundedCornerShape(ThemeRadius.control)
    CompositionLocalProvider(
        LocalOnNoticeFill provides onFill,
        LocalContentColor provides if (onFill) BaeTheme.colors.onNotice else MaterialTheme.colorScheme.onSurface,
    ) {
        Row(
            modifier =
                modifier
                    .fillMaxWidth()
                    .then(if (quiet) Modifier else Modifier.background(tone.noticeFill, shape))
                    .then(
                        if (quiet) {
                            Modifier.padding(vertical = ThemeSpace.related + ThemeSpace.line)
                        } else {
                            Modifier.padding(ThemeSpace.group)
                        },
                    ),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            if (tone.icon(outline = false) != null) {
                FirstLineGlyph(tone, outline = quiet)
            }
            content()
        }
    }
}

/** Where a notice's words start, past its glyph: what a list of notices insets its dividers by. */
internal val NoticeTextInset = ThemeIcon.medium + ThemeSpace.related

/** The tone's glyph in a [FirstLineBox], so it centres on the first line. */
@Composable
private fun FirstLineGlyph(
    tone: StatusTone,
    outline: Boolean,
) {
    FirstLineBox { StatusGlyph(tone, outline = outline) }
}

/**
 * A box one body line tall, its [content] centred in it; the content may overhang the box. A
 * notice's glyph and buttons sit in one, on the title's line.
 */
@Composable
internal fun FirstLineBox(content: @Composable BoxScope.() -> Unit) {
    val measurer = rememberTextMeasurer()
    val style = ThemeText.body.style
    val lineHeight =
        with(LocalDensity.current) {
            remember(measurer, style) { measurer.measure("A", style).size.height }.toDp()
        }
    Box(Modifier.height(lineHeight), contentAlignment = Alignment.Center) {
        Box(Modifier.wrapContentHeight(unbounded = true), content = content)
    }
}

/** A line saying something failed. */
@Composable
fun ErrorText(
    message: String,
    modifier: Modifier = Modifier,
    maxLines: Int = Int.MAX_VALUE,
) {
    Text(
        text = message,
        style = ThemeText.body.style,
        color = BaeTheme.colors.danger,
        maxLines = maxLines,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier,
    )
}
