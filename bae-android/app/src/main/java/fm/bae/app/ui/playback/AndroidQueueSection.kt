package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidEyebrow
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.IconTone

/**
 * A queue section's label with its Clear, whose content description is [clearDescription] because a
 * screen reader reads the button alone. A section that can shuffle takes [shuffled], null where it
 * can't, with [onShuffle].
 */
@Composable
internal fun AndroidQueueSection(
    title: String,
    clearDescription: String,
    onClear: () -> Unit,
    shuffled: Boolean? = null,
    onShuffle: () -> Unit = {},
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = ThemeSpace.edge, end = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AndroidEyebrow(text = title, modifier = Modifier.weight(1f), spaceAbove = 0.dp)
        AndroidButton(
            kind = ButtonKind.TEXT,
            onClick = onClear,
            modifier = Modifier.semantics { contentDescription = clearDescription },
        ) {
            Text(stringResource(R.string.queue_clear))
        }
        if (shuffled != null) {
            AndroidIconButton(
                icon = SymbolIcons.Shuffle,
                contentDescription =
                    stringResource(if (shuffled) R.string.queue_shuffle_off else R.string.queue_shuffle_on),
                onClick = onShuffle,
                tone = if (shuffled) IconTone.ACCENT else IconTone.MUTED,
                glyph = ThemeIcon.medium,
            )
        }
    }
}
