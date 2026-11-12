package fm.bae.app.ui.library

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.lerp
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.lerp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidMenu
import kotlin.math.roundToInt

/** The header's menus; one is open at a time. */
internal enum class LibraryHeaderMenu {
    /** The heading's own menu, switching between the library's views. */
    VIEWS,

    /** The ⋮ menu: the view's sort, Shuffle library and Settings. */
    MORE,
}

/** How far the list scrolls while the heading shrinks from full size into the slim bar, as on macOS. */
private val COLLAPSE_DISTANCE = 64.dp

/** How long the heading takes to follow the scroll, as on macOS. */
private const val COLLAPSE_ANIMATION_MILLIS = 150

/**
 * How far the heading has collapsed at a scroll position: 0 at the top of the
 * list, following the scroll to 1 over [distance], and 1 past the first item.
 * Once the scroll stops part way, the heading takes the nearer end, so it never
 * rests half-size; the next scroll follows the position again.
 */
internal fun headingCollapse(
    firstIndex: Int,
    firstOffset: Int,
    distance: Float,
    scrolling: Boolean,
): Float {
    val position = if (firstIndex > 0) 1f else (firstOffset / distance).coerceIn(0f, 1f)
    return if (scrolling) position else position.roundToInt().toFloat()
}

@Composable
internal fun rememberHeadingCollapse(grid: LazyGridState): State<Float> =
    rememberHeadingCollapse(grid, { grid.firstVisibleItemIndex }, { grid.firstVisibleItemScrollOffset })

@Composable
internal fun rememberHeadingCollapse(list: LazyListState): State<Float> =
    rememberHeadingCollapse(list, { list.firstVisibleItemIndex }, { list.firstVisibleItemScrollOffset })

@Composable
private fun rememberHeadingCollapse(
    scroll: ScrollableState,
    firstIndex: () -> Int,
    firstOffset: () -> Int,
): State<Float> {
    val distance = with(LocalDensity.current) { COLLAPSE_DISTANCE.toPx() }
    val target by remember(scroll, distance) {
        derivedStateOf { headingCollapse(firstIndex(), firstOffset(), distance, scroll.isScrollInProgress) }
    }
    return animateFloatAsState(
        targetValue = target,
        animationSpec = tween(COLLAPSE_ANIMATION_MILLIS),
        label = "heading collapse",
    )
}

/**
 * The library home's header: the view's name as the page's heading, whose menu
 * switches views, and the actions at its end. It shrinks from the full heading
 * at the top of the list into a slim bar by [collapse], 0 to 1.
 */
@Composable
internal fun LibraryHeader(
    state: LibraryBrowserState,
    collapse: () -> Float,
    onShuffleLibrary: () -> Unit,
    onSettings: () -> Unit,
    syncActivity: @Composable () -> Unit,
) {
    val progress = collapse()
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .background(lerp(MaterialTheme.colorScheme.background, MaterialTheme.colorScheme.surface, progress))
                .padding(
                    start = ThemeSpace.edge,
                    end = ThemeSpace.inline,
                    top = lerp(ThemeSpace.related, ThemeSpace.inline, progress),
                    bottom = lerp(0.dp, ThemeSpace.inline, progress),
                ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LibraryViewHeading(state = state, progress = progress, modifier = Modifier.weight(1f))
        IconButton(onClick = { state.searchOpen = true }) {
            Icon(imageVector = Icons.Filled.Search, contentDescription = stringResource(R.string.search))
        }
        syncActivity()
        LibraryMoreMenu(state = state, onShuffleLibrary = onShuffleLibrary, onSettings = onSettings)
    }
}

/** The view's name and the chevron that says it opens a menu of the views. */
@Composable
private fun LibraryViewHeading(
    state: LibraryBrowserState,
    progress: Float,
    modifier: Modifier,
) {
    Box(modifier = modifier) {
        Row(
            modifier =
                Modifier
                    .clip(RoundedCornerShape(ThemeRadius.control))
                    .clickable(
                        onClickLabel = stringResource(R.string.library_switch_view),
                        role = Role.DropdownList,
                    ) { state.openMenu = LibraryHeaderMenu.VIEWS }
                    .semantics { heading() },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = stringResource(state.mode.label),
                style = lerp(ThemeText.display.style, ThemeText.title.style, progress),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Spacer(modifier = Modifier.width(ThemeSpace.compact))
            Icon(
                imageVector = Icons.Filled.KeyboardArrowDown,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(lerp(ThemeIcon.medium, ThemeIcon.small, progress)),
            )
        }
        AndroidMenu(
            expanded = state.openMenu == LibraryHeaderMenu.VIEWS,
            onDismissRequest = { state.openMenu = null },
            minWidth = 180.dp,
        ) {
            LibraryViewMenuItems(state)
        }
    }
}

/**
 * The heading's menu: the library's views, the one on screen chosen. Its own
 * composable so a screenshot scene can show the menu's contents, which
 * layoutlib can't draw in the menu's popup.
 */
@Composable
internal fun LibraryViewMenuItems(state: LibraryBrowserState) {
    LibraryBrowserMode.entries.forEach { mode ->
        AndroidMenuItem(
            label = stringResource(mode.label),
            selected = mode == state.mode,
            onClick = {
                state.mode = mode
                state.openMenu = null
            },
        )
    }
}

/** One of a menu's choices, the chosen one in the accent color. */
@Composable
internal fun AndroidMenuItem(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    DropdownMenuItem(
        text = {
            Text(
                text = label,
                color = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                fontWeight = if (selected) FontWeight.SemiBold else null,
            )
        },
        onClick = onClick,
        modifier = Modifier.semantics { this.selected = selected },
    )
}
