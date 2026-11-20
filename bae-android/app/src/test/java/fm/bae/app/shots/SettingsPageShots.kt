package fm.bae.app.shots

import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.settings.AndroidSettingsAbout
import fm.bae.app.ui.settings.AndroidSettingsAppearance
import fm.bae.app.ui.settings.AndroidSettingsList
import fm.bae.app.ui.settings.AndroidSettingsPlayback
import fm.bae.app.ui.settings.AndroidSettingsTransfers
import fm.bae.app.ui.settings.SettingsSummaries
import org.junit.Test
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat
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
                ScreenScene(SYSTEM_APPEARANCE) { AndroidSettingsAppearance(onBack = {}) }
            }
        }

    /** The Playback page with pausing between sides on, never resuming by itself, restoring on launch, no Play on. */
    @Test
    fun androidSettingsPlayback() =
        shot("android.settings.playback") {
            ScreenScene {
                AndroidSettingsPlayback(
                    pauseBetweenSides = true,
                    countdown = BridgeSidePauseCountdown.OFF,
                    restoreOnLaunch = true,
                    playOnEnabled = false,
                    receivers = { BridgeCastReceiverList(casting = null, found = emptyList(), isEmpty = true) },
                    connecting = { null },
                    transcodeFormat = BridgeCastTranscodeFormat.MP3,
                    onBack = {},
                    onPauseBetweenSides = {},
                    onCountdown = {},
                    onRestoreOnLaunch = {},
                    onPlayOnEnabledChange = {},
                    onPlayOn = {},
                    onDisconnect = {},
                    onTranscodeFormatChange = {},
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
}

private fun summaries(library: String) =
    SettingsSummaries(
        appearance = "System · Midnight · Pink",
        library = library,
        playback = "Pauses between sides · Restores on launch",
        transfers = "3 pins at once",
        storage = "3.9 GB on this phone",
        about = "Version 0.0-dev",
    )
