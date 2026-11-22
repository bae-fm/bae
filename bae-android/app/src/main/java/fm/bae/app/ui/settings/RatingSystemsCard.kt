package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.DialogAction
import uniffi.bae_bridge.BridgeRatedItems
import uniffi.bae_bridge.BridgeRatingSystem
import uniffi.bae_bridge.BridgeRatingSystems

/**
 * The Ratings section of the Library page: how the library rates its albums and its tracks, each Stars, Heart or None,
 * for everyone in the library. An admin's rows open the choice, which [onChange] writes; a member's rows show the
 * system as a plain value and open nothing.
 */
@Composable
internal fun RatingSystemsCard(
    systems: BridgeRatingSystems,
    canChange: Boolean,
    onChange: (BridgeRatedItems, BridgeRatingSystem) -> Unit,
) {
    AndroidSection(stringResource(R.string.ratings_title))
    AndroidCard {
        RatingSystemRow(
            title = stringResource(R.string.search_section_albums),
            selected = systems.albums,
            canChange = canChange,
            onChoose = { onChange(BridgeRatedItems.ALBUMS, it) },
        )
        RatingSystemRow(
            title = stringResource(R.string.search_section_tracks),
            selected = systems.tracks,
            canChange = canChange,
            onChoose = { onChange(BridgeRatedItems.TRACKS, it) },
        )
    }
    AndroidNote(stringResource(R.string.ratings_note))
}

/** A row naming the [selected] system. For an admin it opens a dialog of the three systems. */
@Composable
private fun RatingSystemRow(
    title: String,
    selected: BridgeRatingSystem,
    canChange: Boolean,
    onChoose: (BridgeRatingSystem) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    AndroidRow(
        title = title,
        subtitle = stringResource(selected.label),
        onClick = if (canChange) ({ open = true }) else null,
    )
    if (open) {
        AndroidDialog(
            onDismissRequest = { open = false },
            title = title,
            content = {
                Column(modifier = Modifier.selectableGroup()) {
                    BridgeRatingSystem.entries.forEach { choice ->
                        AndroidRadio(
                            label = stringResource(choice.label),
                            checked = choice == selected,
                            onClick = {
                                open = false
                                onChoose(choice)
                            },
                        )
                    }
                }
            },
            dismiss = DialogAction(stringResource(R.string.cancel), { open = false }),
        )
    }
}

/** The system's name in Library settings: Hidden is the choice named None. */
internal val BridgeRatingSystem.label: Int
    @StringRes get() =
        when (this) {
            BridgeRatingSystem.STARS -> R.string.rating_system_stars
            BridgeRatingSystem.HEART -> R.string.rating_system_heart
            BridgeRatingSystem.HIDDEN -> R.string.rating_system_none
        }
