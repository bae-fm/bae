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
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.LocalAppearancePalette
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.LocalAppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone
import kotlinx.coroutines.launch
import java.io.IOException

@Composable
internal fun AppearanceSection() {
    val store = LocalAppearanceStore.current
    val preferences by store.preferences.collectAsState()
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val save: (suspend () -> Unit) -> Unit = { change ->
        scope.launch {
            try {
                change()
                error = null
            } catch (failure: IOException) {
                error = failure.localizedMessage ?: failure.toString()
            }
        }
    }
    Column(
        modifier = Modifier.fillMaxWidth().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(stringResource(R.string.appearance_title), style = MaterialTheme.typography.titleSmall)
        AppearancePicker(
            title = stringResource(R.string.appearance_mode),
            selected = preferences.mode,
            choices = AppearanceMode.entries,
            label = { it.label },
            onSelect = { save { store.setMode(it) } },
        )
        Text(stringResource(R.string.appearance_accent), style = MaterialTheme.typography.bodyMedium)
        AccentPicker(preferences.accent) { accent -> save { store.setAccent(accent) } }
        AppearancePicker(
            title = stringResource(R.string.appearance_tone),
            selected = preferences.tone,
            choices = SurfaceTone.entries,
            label = { it.label },
            onSelect = { save { store.setTone(it) } },
        )
        error?.let {
            Text(
                stringResource(R.string.appearance_save_failed, it),
                color = MaterialTheme.colorScheme.error,
            )
        }
    }
}

@Composable
private fun AccentPicker(
    selectedAccent: AccentChoice,
    onSelect: (AccentChoice) -> Unit,
) {
    val palette = LocalAppearancePalette.current
    Row(modifier = Modifier.fillMaxWidth().selectableGroup()) {
        AccentChoice.entries.forEach { accent ->
            val label = stringResource(accent.label)
            val selected = selectedAccent == accent
            Box(
                modifier =
                    Modifier
                        .weight(1f)
                        .size(44.dp)
                        .selectable(selected, role = Role.RadioButton) { onSelect(accent) }
                        .semantics { contentDescription = label },
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    modifier = Modifier.size(24.dp).background(palette.accentFill(accent), CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        Icons.Default.Check,
                        contentDescription = null,
                        tint = Color.White,
                        modifier = Modifier.size(14.dp).alpha(if (selected) 1f else 0f),
                    )
                }
            }
        }
    }
}

@Composable
private fun <T> AppearancePicker(
    title: String,
    selected: T,
    choices: List<T>,
    label: (T) -> Int,
    onSelect: (T) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, modifier = Modifier.weight(1f))
        Box {
            TextButton(onClick = { expanded = true }) { Text(stringResource(label(selected))) }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                choices.forEach { choice ->
                    DropdownMenuItem(
                        text = { Text(stringResource(label(choice))) },
                        onClick = {
                            expanded = false
                            onSelect(choice)
                        },
                    )
                }
            }
        }
    }
}

private val AppearanceMode.label: Int
    @StringRes get() =
        when (this) {
            AppearanceMode.SYSTEM -> R.string.appearance_system
            AppearanceMode.LIGHT -> R.string.appearance_light
            AppearanceMode.DARK -> R.string.appearance_dark
        }

private val SurfaceTone.label: Int
    @StringRes get() =
        when (this) {
            SurfaceTone.NEUTRAL -> R.string.appearance_neutral
            SurfaceTone.SLATE -> R.string.appearance_slate
            SurfaceTone.PLUM -> R.string.appearance_plum
        }

private val AccentChoice.label: Int
    @StringRes get() =
        when (this) {
            AccentChoice.BLUE -> R.string.appearance_blue
            AccentChoice.INDIGO -> R.string.appearance_indigo
            AccentChoice.PURPLE -> R.string.appearance_purple
            AccentChoice.PINK -> R.string.appearance_pink
            AccentChoice.RED -> R.string.appearance_red
            AccentChoice.AMBER -> R.string.appearance_amber
            AccentChoice.GREEN -> R.string.appearance_green
            AccentChoice.TEAL -> R.string.appearance_teal
        }
