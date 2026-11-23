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
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSheetHeading
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeRatedItems
import uniffi.bae_bridge.BridgeRatingSystem
import uniffi.bae_bridge.BridgeRatingSystems

/**
 * The Ratings section of the Library page: how the library rates its albums and its tracks, each Stars, Heart or None,
 * for everyone in the library. It shows nothing until [systems] answers. An admin's rows open the choice, which
 * [onChange] writes; a member's rows show the system as a plain value and open nothing.
 */
@Composable
internal fun RatingSystemsCard(
    systems: () -> BridgeRatingSystems?,
    onChange: suspend (BridgeRatedItems, BridgeRatingSystem) -> Unit,
) {
    val shown = systems() ?: return
    AndroidSection(stringResource(R.string.ratings_title))
    AndroidCard {
        RatingSystemRow(
            title = stringResource(R.string.search_section_albums),
            selected = shown.albums,
            canChange = shown.canChange,
            onChoose = { onChange(BridgeRatedItems.ALBUMS, it) },
        )
        RatingSystemRow(
            title = stringResource(R.string.search_section_tracks),
            selected = shown.tracks,
            canChange = shown.canChange,
            onChoose = { onChange(BridgeRatedItems.TRACKS, it) },
        )
    }
    AndroidNote(stringResource(R.string.ratings_note))
}

/**
 * A row naming the [selected] system. For an admin it opens a sheet of the three systems, which closes when the person
 * picks one. The row shows the person's pick until [onChoose] returns.
 */
@Composable
private fun RatingSystemRow(
    title: String,
    selected: BridgeRatingSystem,
    canChange: Boolean,
    onChoose: suspend (BridgeRatingSystem) -> Unit,
) {
    var open by remember(canChange) { mutableStateOf(false) }
    var picked by remember { mutableStateOf<BridgeRatingSystem?>(null) }
    val scope = rememberScreenScope()
    val shown = picked ?: selected
    AndroidRow(
        title = title,
        subtitle = stringResource(shown.label),
        onClick = if (canChange && picked == null) ({ open = true }) else null,
    )
    if (open) {
        AndroidSheet(onDismiss = { open = false }, heading = AndroidSheetHeading(title = title)) {
            Column(modifier = Modifier.selectableGroup()) {
                BridgeRatingSystem.entries.forEach { choice ->
                    AndroidRadio(
                        label = stringResource(choice.label),
                        checked = choice == shown,
                        onClick = {
                            open = false
                            picked = choice
                            scope.launch {
                                try {
                                    onChoose(choice)
                                } finally {
                                    picked = null
                                }
                            }
                        },
                    )
                }
            }
        }
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
