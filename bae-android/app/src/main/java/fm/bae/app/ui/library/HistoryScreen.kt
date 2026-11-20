package fm.bae.app.ui.library

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.creditLineText
import fm.bae.app.data.HistoryOf
import fm.bae.app.data.HistoryPageStore
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidListRow
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.AndroidTopBar
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeHistoryPlay
import uniffi.bae_bridge.BridgeHistoryShown
import uniffi.bae_bridge.BridgeListCountExtra
import java.time.LocalDate
import java.util.TimeZone

/**
 * History: the plays this device holds, newest first, under the day they were played, everyone's or one person's.
 * The end of the top bar names whose plays it shows and opens the choice, which the list's count carries.
 */
@Composable
internal fun HistoryScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
    backLabel: String,
) {
    var person by rememberSaveable { mutableStateOf<String?>(null) }
    val page = rememberHistoryPage(session, HistoryOf(person))
    val menu = (page.countExtra as? BridgeListCountExtra.History)
    var menuOpen by remember { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxSize()) {
        HistoryTopBar(
            onBack = onBack,
            backLabel = backLabel,
            menu = menu,
            menuOpen = menuOpen,
            onMenuOpen = { menuOpen = it },
            onChoose = { chosen ->
                menuOpen = false
                person = chosen
            },
        )
        HistoryContent(session = session, page = page, shown = menu?.shown, listState = rememberLazyListState())
    }
}

/** History's bar: the way back, the title, and, once the list's count says whose plays it can show, that choice. */
@Composable
internal fun HistoryTopBar(
    onBack: () -> Unit,
    backLabel: String,
    menu: BridgeListCountExtra.History?,
    menuOpen: Boolean,
    onMenuOpen: (Boolean) -> Unit,
    onChoose: (String?) -> Unit,
) {
    AndroidTopBar(
        onBack = onBack,
        backLabel = backLabel,
        title = stringResource(R.string.history),
        actions = {
            if (menu != null) {
                WhoseMenu(menu = menu, open = menuOpen, onOpen = onMenuOpen, onChoose = onChoose)
            }
        },
    )
}

@Composable
private fun rememberHistoryPage(
    session: OpenLibrary,
    of: HistoryOf,
): HistoryPageStore {
    val page = session.browserPages.history
    val screenRows = rememberScreenRows()
    DisposableEffect(page, of) {
        page.screenRows = screenRows
        page.activate(of)
        onDispose { page.deactivate(of) }
    }
    return page
}

/** The choice of whose plays History shows, at the end of the top bar: the current choice with a ⌄. */
@Composable
private fun RowScope.WhoseMenu(
    menu: BridgeListCountExtra.History,
    open: Boolean,
    onOpen: (Boolean) -> Unit,
    onChoose: (String?) -> Unit,
) {
    val shown = menu.shown
    val label =
        when (shown) {
            BridgeHistoryShown.Everyone -> stringResource(R.string.history_everyone)
            is BridgeHistoryShown.Person -> shown.name
        }
    Box {
        Row(
            modifier =
                Modifier
                    .clickable(role = Role.DropdownList) { onOpen(true) }
                    .padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = label, style = ThemeText.rowTitle.style, color = MaterialTheme.colorScheme.primary)
            Icon(
                imageVector = Icons.Filled.KeyboardArrowDown,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        AndroidMenu(expanded = open, onDismissRequest = { onOpen(false) }) {
            WhoseMenuItems(menu = menu, onChoose = onChoose)
        }
    }
}

/** What the choice of whose plays holds: Everyone, then the library's people, the chosen one marked. */
@Composable
internal fun WhoseMenuItems(
    menu: BridgeListCountExtra.History,
    onChoose: (String?) -> Unit,
) {
    AndroidMenuItem(
        label = stringResource(R.string.history_everyone),
        selected = menu.shown == BridgeHistoryShown.Everyone,
        onClick = { onChoose(null) },
    )
    menu.people.forEach { person ->
        AndroidMenuItem(label = person.name, selected = person.chosen, onClick = { onChoose(person.person) })
    }
}

/** History's plays as rows under their days' headings, each row on screen keeping the list loading it. */
@Composable
private fun HistoryContent(
    session: OpenLibrary,
    page: HistoryPageStore,
    shown: BridgeHistoryShown?,
    listState: LazyListState,
) {
    val empty =
        when (shown) {
            is BridgeHistoryShown.Person -> stringResource(R.string.history_empty_person, shown.name)
            else -> stringResource(R.string.history_empty)
        }
    LibraryPageContent(session = session, page = page, emptyMessage = empty) {
        LaunchedEffect(page, listState) {
            snapshotFlow {
                val visible = listState.layoutInfo.visibleItemsInfo
                (visible.firstOrNull()?.index ?: 0) to (visible.lastOrNull()?.index ?: 0)
            }.distinctUntilChanged().collect { (first, last) -> page.reportVisibleRange(first, last) }
        }
        HistoryPlays(count = page.totalCount, rowAt = page::rowAt, listState = listState)
    }
}

/** History's plays as rows under their days' headings; [rowAt] gives null for a row the list has not read yet. */
@Composable
private fun HistoryPlays(
    count: Int,
    rowAt: (Int) -> BridgeHistoryPlay?,
    listState: LazyListState,
) {
    val context = LocalContext.current
    LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
        items(count, key = { index -> rowAt(index)?.id ?: "play-slot-$index" }) { index ->
            rowAt(index)?.let { play -> PlayRow(context, play) }
        }
    }
}

@Composable
private fun PlayRow(
    context: Context,
    play: BridgeHistoryPlay,
) {
    Column {
        if (play.opensDay) {
            Box(modifier = Modifier.padding(horizontal = ThemeSpace.edge)) {
                AndroidSection(text = dayHeading(context, play.playedOn, LocalDate.now()))
            }
        }
        AndroidListRow(
            title = play.title,
            subtitle = context.creditLineText(play.artistLine),
            trailing = timeOf(context, play.startedAt, TimeZone.getDefault()),
            art = play.cover,
            onClick = null,
            who = play.person.face(),
        )
    }
}
