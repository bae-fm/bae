package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.playback.CastReceiverDetailsRows
import fm.bae.app.ui.playback.CastReceiverMenuItems
import fm.bae.app.ui.playback.CastReceiverRole
import fm.bae.app.ui.settings.AndroidSettingsAbout
import fm.bae.app.ui.settings.AndroidSettingsAppearance
import fm.bae.app.ui.settings.AndroidSettingsCasting
import fm.bae.app.ui.settings.AndroidSettingsList
import fm.bae.app.ui.settings.AndroidSettingsPlayback
import fm.bae.app.ui.settings.AndroidSettingsTransfers
import fm.bae.app.ui.settings.SettingsSummaries
import org.junit.Test
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeRendererKind
import uniffi.bae_bridge.BridgeSidePauseCountdown

/** The shots of the ready android.settings.* boards for the Settings list and the pages it opens. */
class SettingsPageShots : BoardShot() {
    /** The Settings list over a library that synced. */
    @Test
    fun androidSettingsList() =
        shot("android.settings.list") {
            ScreenScene { AndroidSettingsList(summaries("Home music · Synced"), {}, {}, {}) }
        }

    /** The Settings list over a library whose phone is offline. */
    @Test
    fun androidSettingsListLibraryOffline() =
        shot("android.settings.list.library-offline") { ScreenScene { AndroidSettingsList(summaries("Home music · Offline"), {}, {}, {}) } }

    /** The Settings list over a library whose cloud is full. */
    @Test
    fun androidSettingsListSyncProblem() =
        shot("android.settings.list.sync-problem") { ScreenScene { AndroidSettingsList(summaries("Home music · Cloud full"), {}, {}, {}) } }

    /** The About page: the sheep over the version. */
    @Test
    fun androidSettingsAbout() = shot("android.settings.about") { ScreenScene { AndroidSettingsAbout(version = "0.0-dev", onBack = {}) } }

    /** The Appearance page on a phone that follows the system, in Midnight and Pink. */
    @Test
    fun androidSettingsAppearance() =
        shot("android.settings.appearance") {
            CompositionLocalProvider(LocalBugReports provides BugReports()) {
                SystemLook { ScreenScene { AndroidSettingsAppearance(onBack = {}) } }
            }
        }

    /** The Playback page with pausing between sides on, never resuming by itself, restoring on launch. */
    @Test
    fun androidSettingsPlayback() =
        shot("android.settings.playback") {
            ScreenScene {
                AndroidSettingsPlayback(
                    pauseBetweenSides = true,
                    countdown = BridgeSidePauseCountdown.OFF,
                    restoreOnLaunch = true,
                    onBack = {},
                    onPauseBetweenSides = {},
                    onCountdown = {},
                    onRestoreOnLaunch = {},
                )
            }
        }

    /** The Transfers page with pins waiting for Wi-Fi off, three pins at once. */
    @Test
    fun androidSettingsTransfers() =
        shot("android.settings.transfers") {
            ScreenScene {
                AndroidSettingsTransfers(
                    unmeteredOnly = LiveValue.Current(false),
                    concurrency = 3u,
                    limit = 8u,
                    onBack = {},
                    onSetUnmeteredOnly = {},
                    onSelectConcurrency = {},
                )
            }
        }

    /** The Casting page with receivers found: one cast to, two more. */
    @Test
    fun androidSettingsCastingFound() =
        shot("android.settings.casting.found") {
            ScreenScene { Casting(enabled = true, receivers = receivers()) }
        }

    /** The Casting page while it looks for receivers. */
    @Test
    fun androidSettingsCastingLooking() =
        shot("android.settings.casting.looking") {
            ScreenScene { Casting(enabled = true, receivers = BridgeCastReceiverList(casting = null, found = emptyList())) }
        }

    /** The Casting page with casting off. */
    @Test
    fun androidSettingsCastingOff() =
        shot("android.settings.casting.off") {
            ScreenScene { Casting(enabled = false, receivers = BridgeCastReceiverList(casting = null, found = emptyList())) }
        }

    /** The Details sheet of the receiver cast to, over the Casting page. */
    @Test
    fun androidSettingsCastingReceiverDetails() =
        shot("android.settings.casting.receiver-details") {
            val casting = checkNotNull(receivers().casting)
            SheetScene(casting.name, behind = { Casting(enabled = true, receivers = receivers()) }) {
                CastReceiverDetailsRows(casting)
            }
        }

    /** The menu of the receiver cast to, open over the page: Details, then Disconnect. */
    @Test
    fun androidSettingsCastingReceiverMenu() =
        shot("android.settings.casting.receiver-menu") {
            ScreenScene {
                Box(modifier = Modifier.fillMaxSize()) {
                    Casting(enabled = true, receivers = receivers())
                    Surface(
                        shape = MenuDefaults.shape,
                        color = MenuDefaults.containerColor,
                        tonalElevation = MenuDefaults.TonalElevation,
                        shadowElevation = MenuDefaults.ShadowElevation,
                        modifier = Modifier.align(Alignment.TopEnd).padding(top = MENU_TOP, end = ThemeSpace.edge),
                    ) {
                        Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(MENU_WIDTH)) {
                            CastReceiverMenuItems(role = CastReceiverRole.Casting(onDisconnect = {}), onDetails = {}, close = {})
                        }
                    }
                }
            }
        }
}

private val MENU_TOP = 296.dp
private val MENU_WIDTH = 200.dp

private fun summaries(library: String) =
    SettingsSummaries(
        appearance = "System · Midnight · Pink",
        library = library,
        playback = "Pauses between sides · Restores on launch",
        transfers = "3 pins at once",
        storage = "3.9 GB on this phone",
        casting = "Off",
        about = "Version 0.0-dev",
    )

@Composable
private fun Casting(
    enabled: Boolean,
    receivers: BridgeCastReceiverList,
) = AndroidSettingsCasting(
    enabled = enabled,
    receivers = receivers,
    connectingName = null,
    transcodeFormat = BridgeCastTranscodeFormat.MP3,
    onBack = {},
    onCast = {},
    onDisconnect = {},
    onEnabledChange = {},
    onTranscodeFormatChange = {},
)

/** Living Room cast to, then the rest as core lists them. */
private fun receivers() =
    BridgeCastReceiverList(
        casting = BridgeCastReceiver("cast-living-room", "Living Room", BridgeRendererKind.CAST, "Google Nest Audio", "192.168.1.42"),
        found =
            listOf(
                BridgeCastReceiver("cast-kitchen", "Kitchen", BridgeRendererKind.AIR_PLAY, "HomePod mini", "192.168.1.63"),
                BridgeCastReceiver("cast-den", "Den receiver", BridgeRendererKind.DLNA, null, "192.168.1.57"),
            ),
    )
