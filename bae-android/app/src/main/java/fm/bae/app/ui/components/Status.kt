package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.StatusRingIcons
import fm.bae.app.ui.appearance.SymbolIcons
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
    get() = icon != null

/** The glyph a status in this tone draws: the outline of a circle with its mark. */
private val StatusTone.icon: ImageVector?
    get() =
        when (this) {
            StatusTone.INFO -> SymbolIcons.Info
            StatusTone.SUCCESS -> SymbolIcons.CheckCircle
            StatusTone.WARNING, StatusTone.DANGER -> SymbolIcons.Error
            StatusTone.NEUTRAL, StatusTone.ACCENT, StatusTone.ACTIVITY -> null
        }

/** The glyph a notice in this tone leads with: a ring with its mark, as the design system draws it. */
private val StatusTone.ringIcon: ImageVector?
    get() =
        when (this) {
            StatusTone.INFO -> StatusRingIcons.Info
            StatusTone.SUCCESS -> StatusRingIcons.Success
            StatusTone.WARNING, StatusTone.DANGER -> StatusRingIcons.Bang
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
    muted: Boolean = false,
) {
    tone.icon?.let { icon ->
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
 * How a notice sits on its screen: on its tone's fill; with no fill; or, for a
 * view that failed whole, centred in the space it's given (drawn by [AndroidNotice], not by [NoticeFrame]).
 */
enum class NoticeLook { FILLED, QUIET, CENTERED }

/**
 * A notice in its tone: the tone's glyph at the top left, centred on the first
 * line of body text beside it, always as an outline. [NoticeLook.FILLED] draws it on the tone's
 * fill; [NoticeLook.QUIET] on the screen, with no side padding.
 */
@Composable
internal fun NoticeFrame(
    tone: StatusTone,
    modifier: Modifier = Modifier,
    look: NoticeLook = NoticeLook.FILLED,
    content: @Composable ColumnScope.() -> Unit,
) {
    val quiet = look != NoticeLook.FILLED
    val onFill = !quiet && tone.hasNoticeFill
    val shape = RoundedCornerShape(ThemeRadius.control)
    CompositionLocalProvider(
        LocalContentColor provides if (onFill) BaeTheme.colors.onNotice else MaterialTheme.colorScheme.onSurface,
    ) {
        AndroidItem(
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
            lead = if (tone.icon != null) ({ NoticeGlyph(tone) }) else null,
            content = content,
        )
    }
}

/** The ring of a notice's glyph is 15.5dp across, as its board draws it. */
private val NoticeGlyphSize: Dp = 15.5.dp

/** The ring fills 20 of the 24 units of its icon, so the icon's box is 1.2 times its ring. */
private val NoticeGlyphBox: Dp = NoticeGlyphSize * 1.2f

/** The tone's ring with its mark, with its ring [NoticeGlyphSize] across in a box of that width. */
@Composable
private fun NoticeGlyph(tone: StatusTone) {
    Box(Modifier.size(NoticeGlyphSize), contentAlignment = Alignment.Center) {
        NoticeRing(tone, size = NoticeGlyphBox, modifier = Modifier.requiredSize(NoticeGlyphBox))
    }
}

/** The tone's ring with its mark in an icon box [size] across, tinted as the tone's glyph (or muted). */
@Composable
internal fun NoticeRing(
    tone: StatusTone,
    size: Dp,
    modifier: Modifier = Modifier,
    muted: Boolean = false,
) {
    tone.ringIcon?.let { icon ->
        Icon(icon, contentDescription = null, tint = glyphTint(tone, muted), modifier = modifier.size(size))
    }
}

/** Where a notice's words start, past its glyph: what a list of notices insets its dividers by. */
internal val NoticeTextInset = NoticeGlyphSize + ThemeSpace.related

/**
 * A row piece: a glyph ([lead]) level with the first line of its text, then the text ([content]), then
 * [trail], such as a button, level with that line too. A notice's glyph and buttons sit this way on its title's
 * line.
 */
@Composable
internal fun AndroidItem(
    modifier: Modifier = Modifier,
    lead: (@Composable BoxScope.() -> Unit)? = null,
    trail: (@Composable BoxScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        lead?.let { FirstLine(alignment = Alignment.TopCenter, content = it) }
        Column(modifier = Modifier.weight(1f), content = content)
        trail?.let { FirstLine(content = it) }
    }
}

/**
 * A box one body line tall, its [content] placed in it by [alignment]; the content may overhang the box. The
 * lead of an [AndroidItem] sits in one at the top of the first line of its text, the trail centred on it.
 */
@Composable
internal fun FirstLine(
    alignment: Alignment = Alignment.Center,
    content: @Composable BoxScope.() -> Unit,
) {
    val measurer = rememberTextMeasurer()
    val style = ThemeText.body.style
    val lineHeight =
        with(LocalDensity.current) {
            remember(measurer, style) { measurer.measure("A", style).size.height }.toDp()
        }
    Box(Modifier.height(lineHeight), contentAlignment = alignment) {
        Box(Modifier.wrapContentHeight(unbounded = true), content = content)
    }
}

/** A line saying something failed. */
@Composable
fun AndroidErrorLine(
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
