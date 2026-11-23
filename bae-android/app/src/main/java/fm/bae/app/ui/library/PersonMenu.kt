package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.data.PersonStore
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.AndroidMenuDivider
import fm.bae.app.ui.components.AndroidMenuEntry
import fm.bae.app.ui.components.Avatar
import fm.bae.app.ui.components.AvatarFace
import fm.bae.app.ui.components.faceOrTile

/** The side of the person's picture in the header. */
private val PERSON_PICTURE = 32.dp

/**
 * The person's picture, or the initials of their name, which ends the library's header and opens their menu. A person
 * with no profile shows a plain tile. It draws nothing until whether they are an admin has been read.
 */
@Composable
internal fun PersonMenu(
    state: LibraryBrowserState,
    person: PersonStore,
    onAddDevice: () -> Unit,
    onInvite: () -> Unit,
    onPlaylists: () -> Unit,
    onHistory: () -> Unit,
    onSettings: () -> Unit,
) {
    val isAdmin = person.isAdmin ?: return
    PersonButton(
        state = state,
        face = person.profile.faceOrTile(),
        isAdmin = isAdmin,
        onAddDevice = onAddDevice,
        onInvite = onInvite,
        onPlaylists = onPlaylists,
        onHistory = onHistory,
        onSettings = onSettings,
    )
}

/** The person's picture, which opens their menu. */
@Composable
internal fun PersonButton(
    state: LibraryBrowserState,
    face: AvatarFace,
    isAdmin: Boolean,
    onAddDevice: () -> Unit,
    onInvite: () -> Unit,
    onPlaylists: () -> Unit,
    onHistory: () -> Unit,
    onSettings: () -> Unit,
) {
    val close = { state.openMenu = null }
    val choosing: (() -> Unit) -> () -> Unit = { action ->
        {
            close()
            action()
        }
    }
    val yourMenu = stringResource(R.string.your_menu)
    Box {
        // The button names the menu: the picture hides itself from screen readers, since a row beside it names the
        // person.
        AndroidIconButton(
            onClick = { state.openMenu = LibraryHeaderMenu.PERSON },
            modifier = Modifier.semantics { contentDescription = yourMenu },
        ) {
            Avatar(face = face, size = PERSON_PICTURE)
        }
        AndroidMenu(
            expanded = state.openMenu == LibraryHeaderMenu.PERSON,
            onDismissRequest = close,
        ) {
            PersonMenuItems(
                isAdmin = isAdmin,
                onAddDevice = choosing(onAddDevice),
                onInvite = choosing(onInvite),
                onPlaylists = choosing(onPlaylists),
                onHistory = choosing(onHistory),
                onSettings = choosing(onSettings),
            )
        }
    }
}

/**
 * What the person's menu holds: Add device and, for an admin, Invite someone, then Playlists and History, then
 * Settings. Its own composable so a screenshot scene can show the menu's contents, which layoutlib can't draw in the
 * menu's popup.
 */
@Composable
internal fun PersonMenuItems(
    isAdmin: Boolean,
    onAddDevice: () -> Unit,
    onInvite: () -> Unit,
    onPlaylists: () -> Unit,
    onHistory: () -> Unit,
    onSettings: () -> Unit,
) {
    AndroidMenuEntry(text = { Text(stringResource(R.string.members_add_device)) }, onClick = onAddDevice)
    if (isAdmin) {
        AndroidMenuEntry(text = { Text(stringResource(R.string.person_menu_invite)) }, onClick = onInvite)
    }
    AndroidMenuDivider()
    AndroidMenuEntry(text = { Text(stringResource(R.string.library_mode_playlists)) }, onClick = onPlaylists)
    AndroidMenuEntry(text = { Text(stringResource(R.string.history)) }, onClick = onHistory)
    AndroidMenuDivider()
    AndroidMenuEntry(text = { Text(stringResource(R.string.settings)) }, onClick = onSettings)
}
