package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.RestorePlaybackPref
import fm.bae.app.ui.appearance.LocalAppearanceStore
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.library.LibraryDestination
import uniffi.bae_bridge.BridgeConfig

/** The sections Settings opens as a page of its own. */
internal enum class SettingsPage {
    Appearance,
    Playback,
    About,
}

/** What each Settings row says under its title: where that setting stands. */
internal data class SettingsSummaries(
    val appearance: String,
    /** The library's name and sync state; whichever has been read so far, none before either has. */
    val library: String?,
    val playback: String,
    val transfers: String,
    val casting: String,
    val about: String,
)

/**
 * Settings as rows on a card, in the order of the Mac's sidebar, each opening a
 * page: those of [SettingsPage] through [onOpen], Library, Transfers and
 * Casting, which are screens of their own, through [onPush].
 */
@Composable
internal fun SettingsMenu(
    summaries: SettingsSummaries,
    onOpen: (SettingsPage) -> Unit,
    onPush: (LibraryDestination) -> Unit,
) {
    Column(modifier = Modifier.padding(ThemeSpace.edge)) {
        AndroidCard {
            AndroidRow(
                title = stringResource(R.string.appearance_title),
                subtitle = summaries.appearance,
                onClick = { onOpen(SettingsPage.Appearance) },
            )
            AndroidRow(
                title = stringResource(R.string.settings_library),
                subtitle = summaries.library,
                onClick = { onPush(LibraryDestination.Library) },
            )
            AndroidRow(
                title = stringResource(R.string.settings_playback),
                subtitle = summaries.playback,
                onClick = { onOpen(SettingsPage.Playback) },
            )
            AndroidRow(
                title = stringResource(R.string.settings_transfers),
                subtitle = summaries.transfers,
                onClick = { onPush(LibraryDestination.Transfers) },
            )
            AndroidRow(
                title = stringResource(R.string.settings_casting),
                subtitle = summaries.casting,
                onClick = { onPush(LibraryDestination.Casting) },
            )
            AndroidRow(
                title = stringResource(R.string.settings_about),
                subtitle = summaries.about,
                onClick = { onOpen(SettingsPage.About) },
            )
        }
    }
}

/**
 * The summaries of a Settings menu for [config], the library's [libraryName] and its [sync] row. The playback summary
 * reads this device's restore-on-launch choice as the menu comes up, which is each time a page closes.
 */
@Composable
internal fun rememberSettingsSummaries(
    config: BridgeConfig,
    libraryName: LibraryNameRead,
    sync: SyncRowSummary,
): SettingsSummaries {
    val context = LocalContext.current
    val resources = LocalResources.current
    val appearance by LocalAppearanceStore.current.preferences.collectAsState()
    val restoresOnLaunch = remember(context) { RestorePlaybackPref.load(context) }
    val appearanceWords = listOf(appearance.mode.label, appearance.tone.label, appearance.accent.label)
    val playbackWords =
        listOfNotNull(
            R.string.settings_summary_pauses_between_sides.takeIf { config.pauseBetweenSides },
            R.string.settings_summary_restores_on_launch.takeIf { restoresOnLaunch },
        )
    val pins = config.maxConcurrentDownloads.toInt()
    return SettingsSummaries(
        appearance = appearanceWords.joinToString(SUMMARY_SEPARATOR) { resources.getString(it) },
        library =
            listOfNotNull((libraryName as? LibraryNameRead.Named)?.name, sync.text)
                .joinToString(SUMMARY_SEPARATOR)
                .ifEmpty { null },
        playback =
            if (playbackWords.isEmpty()) {
                resources.getString(R.string.settings_summary_off)
            } else {
                playbackWords.joinToString(SUMMARY_SEPARATOR) { resources.getString(it) }
            },
        transfers = resources.getQuantityString(R.plurals.transfers_pins_summary, pins, pins),
        casting = resources.getString(onOff(config.castEnabled)),
        about = resources.getString(R.string.settings_about_summary, installedVersion()),
    )
}

@StringRes
private fun onOff(on: Boolean): Int = if (on) R.string.settings_summary_on else R.string.settings_summary_off

private const val SUMMARY_SEPARATOR = " · "

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
