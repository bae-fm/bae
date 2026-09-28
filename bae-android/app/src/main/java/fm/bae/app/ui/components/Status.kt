package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
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

/** The glyph a notice in this tone leads with. */
private val StatusTone.icon: ImageVector?
    get() =
        when (this) {
            StatusTone.INFO -> Icons.Filled.Info
            StatusTone.SUCCESS -> Icons.Filled.CheckCircle
            StatusTone.WARNING -> Icons.Filled.Warning
            StatusTone.DANGER -> Icons.Filled.Error
            StatusTone.NEUTRAL, StatusTone.ACCENT, StatusTone.ACTIVITY -> null
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

/** A notice in its tone: the tone's glyph beside the content, on the tone's fill. */
@Composable
fun Notice(
    tone: StatusTone,
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .background(tone.fill, RoundedCornerShape(ThemeRadius.control))
                .padding(ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        tone.icon?.let { icon ->
            Icon(icon, contentDescription = null, tint = tone.color, modifier = Modifier.size(ThemeIcon.medium))
        }
        content()
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
