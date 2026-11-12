package fm.bae.app.ui.library

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

private val ALBUM_SORT_FIELDS =
    listOf(
        BridgeSortField.TITLE,
        BridgeSortField.ARTIST,
        BridgeSortField.YEAR,
        BridgeSortField.DATE_ADDED,
    )

private val ARTIST_SORT_FIELDS =
    listOf(
        BridgeArtistSortField.NAME,
        BridgeArtistSortField.ALBUM_COUNT,
    )

private val COMPOSER_SORT_FIELDS =
    listOf(
        BridgeComposerSortField.NAME,
        BridgeComposerSortField.WORK_COUNT,
        BridgeComposerSortField.LINKED_RELEASE_COUNT,
    )

@StringRes
private fun BridgeSortField.label(): Int =
    when (this) {
        BridgeSortField.TITLE -> R.string.sort_title
        BridgeSortField.ARTIST -> R.string.sort_artist
        BridgeSortField.YEAR -> R.string.sort_year
        BridgeSortField.DATE_ADDED -> R.string.sort_date_added
    }

@StringRes
private fun BridgeArtistSortField.label(): Int =
    when (this) {
        BridgeArtistSortField.NAME -> R.string.sort_name
        BridgeArtistSortField.ALBUM_COUNT -> R.string.search_section_albums
    }

@StringRes
private fun BridgeComposerSortField.label(): Int =
    when (this) {
        BridgeComposerSortField.NAME -> R.string.sort_name
        BridgeComposerSortField.WORK_COUNT -> R.string.search_section_works
        BridgeComposerSortField.LINKED_RELEASE_COUNT -> R.string.search_section_releases
    }

private fun BridgeSortDirection.reversed(): BridgeSortDirection =
    when (this) {
        BridgeSortDirection.ASCENDING -> BridgeSortDirection.DESCENDING
        BridgeSortDirection.DESCENDING -> BridgeSortDirection.ASCENDING
    }

/**
 * The header's ⋮ and its menu: the view's sort choices under "Sort by" (the
 * albums view's with Group by artist), then Shuffle library and Settings.
 */
@Composable
internal fun LibraryMoreMenu(
    state: LibraryBrowserState,
    onShuffleLibrary: () -> Unit,
    onSettings: () -> Unit,
) {
    val close = { state.openMenu = null }
    Box {
        IconButton(onClick = { state.openMenu = LibraryHeaderMenu.MORE }) {
            Icon(imageVector = Icons.Filled.MoreVert, contentDescription = stringResource(R.string.more))
        }
        DropdownMenu(expanded = state.openMenu == LibraryHeaderMenu.MORE, onDismissRequest = close) {
            LibraryMoreMenuItems(state = state, onShuffleLibrary = onShuffleLibrary, onSettings = onSettings)
        }
    }
}

/**
 * What the ⋮ menu holds; each choice closes the menu. Its own composable so a
 * screenshot scene can show the menu's contents, which layoutlib can't draw in
 * the menu's popup.
 */
@Composable
internal fun LibraryMoreMenuItems(
    state: LibraryBrowserState,
    onShuffleLibrary: () -> Unit,
    onSettings: () -> Unit,
) {
    val close = { state.openMenu = null }
    Text(
        text = stringResource(R.string.sort_by),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier =
            Modifier
                .padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.inline)
                .semantics { heading() },
    )
    ViewSortItems(state = state, close = close)
    HorizontalDivider(modifier = Modifier.padding(vertical = ThemeSpace.related))
    DropdownMenuItem(
        text = { Text(stringResource(R.string.shuffle_library)) },
        onClick = {
            close()
            onShuffleLibrary()
        },
    )
    DropdownMenuItem(
        text = { Text(stringResource(R.string.settings)) },
        onClick = {
            close()
            onSettings()
        },
    )
}

/** The sort choices of the view on screen; choosing one closes the menu. */
@Composable
private fun ViewSortItems(
    state: LibraryBrowserState,
    close: () -> Unit,
) {
    when (state.mode) {
        LibraryBrowserMode.ALBUMS -> {
            val criterion = state.sortCriterion
            SortItems(
                fields = ALBUM_SORT_FIELDS,
                label = { it.label() },
                field = criterion.field,
                direction = criterion.direction,
                onChange = { field, direction ->
                    state.sortCriterion = BridgeSortCriterion(field, direction)
                    close()
                },
            )
            GroupByArtistItem(
                grouped = state.groupAlbumsByArtist,
                onChange = {
                    state.groupAlbumsByArtist = it
                    close()
                },
            )
        }

        LibraryBrowserMode.ARTISTS -> {
            val criterion = state.artistSortCriterion
            SortItems(
                fields = ARTIST_SORT_FIELDS,
                label = { it.label() },
                field = criterion.field,
                direction = criterion.direction,
                onChange = { field, direction ->
                    state.artistSortCriterion = BridgeArtistSortCriterion(field, direction)
                    close()
                },
            )
        }

        LibraryBrowserMode.COMPOSERS -> {
            val criterion = state.composerSortCriterion
            SortItems(
                fields = COMPOSER_SORT_FIELDS,
                label = { it.label() },
                field = criterion.field,
                direction = criterion.direction,
                onChange = { field, direction ->
                    state.composerSortCriterion = BridgeComposerSortCriterion(field, direction)
                    close()
                },
            )
        }
    }
}

/** A view's sort fields, the current one chosen, then the item that reverses the direction it names. */
@Composable
private fun <F> SortItems(
    fields: List<F>,
    label: (F) -> Int,
    field: F,
    direction: BridgeSortDirection,
    onChange: (F, BridgeSortDirection) -> Unit,
) {
    fields.forEach { choice ->
        AndroidMenuItem(
            label = stringResource(label(choice)),
            selected = choice == field,
            onClick = { onChange(choice, direction) },
        )
    }
    val directionLabel =
        when (direction) {
            BridgeSortDirection.ASCENDING -> R.string.sort_ascending
            BridgeSortDirection.DESCENDING -> R.string.sort_descending
        }
    DropdownMenuItem(
        text = { Text(stringResource(directionLabel)) },
        onClick = { onChange(field, direction.reversed()) },
    )
}

/** Group by artist, its check showing whether the grid is grouped. */
@Composable
internal fun GroupByArtistItem(
    grouped: Boolean,
    onChange: (Boolean) -> Unit,
) {
    DropdownMenuItem(
        text = { Text(stringResource(R.string.group_by_artist)) },
        onClick = { onChange(!grouped) },
        modifier = Modifier.semantics { toggleableState = ToggleableState(grouped) },
        trailingIcon = {
            Icon(
                imageVector = Icons.Filled.Check,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.alpha(if (grouped) 1f else 0f),
            )
        },
    )
}
