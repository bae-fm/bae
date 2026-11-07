package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.RowCard
import fm.bae.app.ui.library.LibraryDestination

/** The sections Settings opens as a page of its own, with the settings top bar. */
internal enum class SettingsPage(
    @StringRes val title: Int,
) {
    Appearance(R.string.appearance_title),
    Playback(R.string.settings_playback),
    About(R.string.settings_about),
}

/**
 * Settings as rows on a card, in the order of the Mac's sidebar, each opening a
 * page: those of [SettingsPage] through [onOpen], Library, Transfers and
 * Casting, which are screens of their own, through [onPush].
 */
@Composable
internal fun SettingsMenu(
    onOpen: (SettingsPage) -> Unit,
    onPush: (LibraryDestination) -> Unit,
) {
    Column(modifier = Modifier.padding(ThemeSpace.edge)) {
        RowCard {
            CardRow(title = stringResource(R.string.appearance_title), onClick = { onOpen(SettingsPage.Appearance) })
            CardRow(
                title = stringResource(R.string.settings_library),
                onClick = { onPush(LibraryDestination.Library) },
            )
            CardRow(title = stringResource(R.string.settings_playback), onClick = { onOpen(SettingsPage.Playback) })
            CardRow(
                title = stringResource(R.string.settings_transfers),
                onClick = { onPush(LibraryDestination.Transfers) },
            )
            CardRow(
                title = stringResource(R.string.settings_casting),
                onClick = { onPush(LibraryDestination.Casting) },
            )
            CardRow(
                title = stringResource(R.string.settings_about),
                subtitle = installedVersion(),
                onClick = { onOpen(SettingsPage.About) },
            )
        }
    }
}

/** The version of bae installed, read from the package rather than BuildConfig so it reflects what is installed. */
@Composable
internal fun installedVersion(): String {
    val context = LocalContext.current
    return remember(context) {
        checkNotNull(context.packageManager.getPackageInfo(context.packageName, 0).versionName) {
            "the app manifest always carries versionName"
        }
    }
}
