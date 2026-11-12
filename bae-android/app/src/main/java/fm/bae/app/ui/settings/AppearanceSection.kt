package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.LocalAppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.colors
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import java.io.IOException

private val logger = BaeLogger("bae.AppearanceSection")

@Composable
internal fun AppearanceSection() {
    val store = LocalAppearanceStore.current
    val preferences by store.preferences.collectAsState()
    val scope = rememberScreenScope()
    val resources = LocalResources.current
    var error by remember { mutableStateOf<DisplayError?>(null) }
    val save: (suspend () -> Unit) -> Unit = { change ->
        scope.launch {
            try {
                change()
                error = null
            } catch (failure: IOException) {
                logger.error("saving appearance failed", failure)
                error = DisplayError(resources.getString(R.string.appearance_save_failed), failure.toString())
            }
        }
    }
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        AndroidCard {
            AppearancePicker(
                title = stringResource(R.string.appearance_theme),
                selected = preferences.mode,
                choices = AppearanceMode.entries,
                label = { it.label },
                onSelect = { save { store.setMode(it) } },
            )
            AppearancePicker(
                title = stringResource(R.string.appearance_tone),
                selected = preferences.tone,
                choices = SurfaceTone.entries,
                label = { it.label },
                onSelect = { save { store.setTone(it) } },
            )
            AndroidRow(
                title = stringResource(R.string.appearance_accent),
                subtitle = stringResource(preferences.accent.label),
            )
            Box(modifier = Modifier.padding(horizontal = ThemeSpace.edge).padding(bottom = ThemeSpace.related)) {
                AndroidAccentPicker(preferences.accent) { accent -> save { store.setAccent(accent) } }
            }
        }
        error?.let { AndroidNotice(it) }
    }
}

@Composable
internal fun AndroidAccentPicker(
    selectedAccent: AccentChoice,
    onSelect: (AccentChoice) -> Unit,
) {
    Row(modifier = Modifier.fillMaxWidth().selectableGroup()) {
        AccentChoice.entries.forEach { accent ->
            val label = stringResource(accent.label)
            val selected = selectedAccent == accent
            Box(
                modifier =
                    Modifier
                        .weight(1f)
                        .size(ThemeSize.hitTarget)
                        .selectable(selected, role = Role.RadioButton) { onSelect(accent) }
                        .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    modifier = Modifier.size(24.dp).background(accent.colors.fill, CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        Icons.Default.Check,
                        contentDescription = null,
                        tint = accent.colors.onFill,
                        modifier = Modifier.size(ThemeIcon.badge).alpha(if (selected) 1f else 0f),
                    )
                }
            }
        }
    }
}

/** A row naming the choice made, which opens a dialog of the choices. */
@Composable
private fun <T> AppearancePicker(
    title: String,
    selected: T,
    choices: List<T>,
    label: (T) -> Int,
    onSelect: (T) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    AndroidRow(title = title, subtitle = stringResource(label(selected)), onClick = { open = true })
    if (open) {
        AndroidDialog(
            onDismissRequest = { open = false },
            title = title,
            content = {
                Column(modifier = Modifier.selectableGroup()) {
                    choices.forEach { choice ->
                        AndroidRadio(
                            label = stringResource(label(choice)),
                            checked = choice == selected,
                            onClick = {
                                open = false
                                onSelect(choice)
                            },
                        )
                    }
                }
            },
            confirmButton = {},
            dismissButton = {
                AndroidButton(
                    kind = ButtonKind.TEXT,
                    onClick = { open = false },
                ) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}

internal val AppearanceMode.label: Int
    @StringRes get() =
        when (this) {
            AppearanceMode.SYSTEM -> R.string.appearance_system
            AppearanceMode.LIGHT -> R.string.appearance_light
            AppearanceMode.DARK -> R.string.appearance_dark
        }

internal val SurfaceTone.label: Int
    @StringRes get() =
        when (this) {
            SurfaceTone.NEUTRAL -> R.string.appearance_neutral
            SurfaceTone.SLATE -> R.string.appearance_slate
            SurfaceTone.PLUM -> R.string.appearance_plum
            SurfaceTone.MIDNIGHT -> R.string.appearance_midnight
            SurfaceTone.FOREST -> R.string.appearance_forest
            SurfaceTone.SAND -> R.string.appearance_sand
        }

internal val AccentChoice.label: Int
    @StringRes get() =
        when (this) {
            AccentChoice.BLUE -> R.string.appearance_blue
            AccentChoice.ORCHID -> R.string.appearance_orchid
            AccentChoice.PINK -> R.string.appearance_pink
            AccentChoice.CORAL -> R.string.appearance_coral
            AccentChoice.GOLD -> R.string.appearance_gold
            AccentChoice.LIME -> R.string.appearance_lime
            AccentChoice.MINT -> R.string.appearance_mint
            AccentChoice.SKY -> R.string.appearance_sky
        }
