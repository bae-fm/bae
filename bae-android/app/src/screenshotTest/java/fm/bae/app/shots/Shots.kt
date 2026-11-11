package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.FailedStep
import fm.bae.app.LocalBugReports
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.R
import fm.bae.app.ScreenFailure
import fm.bae.app.coreString
import fm.bae.app.countLabelsText
import fm.bae.app.data.LiveValue
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.ResetOfferedScreen
import fm.bae.app.ui.ResetUnfinishedScreen
import fm.bae.app.ui.albumdetail.AlbumDetailScene
import fm.bae.app.ui.albumdetail.AndroidDownloadControl
import fm.bae.app.ui.albumdetail.AndroidUploadLine
import fm.bae.app.ui.albumdetail.DeleteAlbumDialog
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSheetBody
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.RemoveLibraryText
import fm.bae.app.ui.components.ScreenTopBar
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.downloads.AndroidDownloadRow
import fm.bae.app.ui.downloads.DownloadsList
import fm.bae.app.ui.library.AndroidErrorBanner
import fm.bae.app.ui.library.LibraryGridScene
import fm.bae.app.ui.library.LibraryHeaderMenu
import fm.bae.app.ui.library.SceneGrid
import fm.bae.app.ui.onboarding.LibraryDetailsRows
import fm.bae.app.ui.onboarding.PasteCodeDialog
import fm.bae.app.ui.onboarding.PasteDialogText
import fm.bae.app.ui.onboarding.RemovalFailedText
import fm.bae.app.ui.onboarding.RestoreText
import fm.bae.app.ui.onboarding.S3KeyDialog
import fm.bae.app.ui.onboarding.S3KeyInput
import fm.bae.app.ui.onboarding.StandingRead
import fm.bae.app.ui.onboarding.WelcomeActions
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeRemovingScene
import fm.bae.app.ui.onboarding.WelcomeScene
import fm.bae.app.ui.playback.FullPlayerScene
import fm.bae.app.ui.playback.MiniPlayerScene
import fm.bae.app.ui.playback.PlayerSceneState
import fm.bae.app.ui.settings.AppearanceSection
import fm.bae.app.ui.settings.FolderChoiceScene
import fm.bae.app.ui.settings.JoinRequestDialogContent
import fm.bae.app.ui.settings.LibrarySettingsContent
import fm.bae.app.ui.settings.PeoplePage
import fm.bae.app.ui.settings.PeopleScene
import fm.bae.app.ui.settings.RestoreCodeScene
import fm.bae.app.ui.settings.S3SetupFormScene
import fm.bae.app.ui.settings.SettingsAboutSection
import fm.bae.app.ui.settings.SettingsMenu
import fm.bae.app.ui.settings.SettingsSummaries
import fm.bae.app.ui.settings.SyncRowSummary
import fm.bae.app.ui.settings.SyncStatusScene
import fm.bae.app.ui.settings.TransfersContent
import fm.bae.app.ui.settings.sceneOtherPerson
import fm.bae.app.ui.signInSettingsProblem
import fm.bae.app.ui.sync.ActivityWords
import fm.bae.app.ui.sync.AndroidActivity
import fm.bae.app.ui.sync.activityProblems
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeDownloadQueueActivity
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryStanding
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeReleaseDownloadStatus
import uniffi.bae_bridge.BridgeStandingState
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone
import uniffi.bae_bridge.BridgeTransferBatch
import uniffi.bae_bridge.BridgeUploadQueueActivity
import uniffi.bae_bridge.BridgeUploadQueueWait
import java.time.Instant

// Screenshot scenes captured by scripts/shots/android.sh. Each function is one
// scene: the plugin renders it to a PNG named after the function, and the script
// copies it to <scene-id>@android.png. A board's preview is named by the board:
// the function is the board name in PascalCase (android.welcome.libraries.none
// is AndroidWelcomeLibrariesNone), and the script checks that the scene id it
// registers spells the same words. Every scene renders a shared, session-free
// composition from the production UI (defined in main next to its @Preview) in
// the app chrome, rendered at a current phone size.

internal const val PHONE_SPEC = "spec:width=411dp,height=914dp,dpi=420"

/** A titled sheet's body on the sheet's surface: the sheet's own window isn't rendered by screenshot tests. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun TitledSheetShot(
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    BaeTheme {
        Surface(color = BottomSheetDefaults.ContainerColor) {
            AndroidSheetBody(title, content)
        }
    }
}

/** A screen of its own, on the screen's background. */
@Composable
internal fun ScreenScene(content: @Composable () -> Unit) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { content() }
    }
}

/** The confirmation to delete an album: Cancel in the screen's text, Delete in red. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun DeleteAlbumDialogScene() {
    BaeTheme { DeleteAlbumDialog(onConfirm = {}, onDismiss = {}) }
}

/** A request to join: the code to check under the line asking to make sure it is showing, Decline in the text colour. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun JoinRequestDialogScene() {
    BaeTheme {
        JoinRequestDialogContent(
            title = "Dan",
            confirmation = "412 778",
            onApprove = {},
            onDecline = {},
            onClose = {},
        )
    }
}

/** The library home at the top of the list: the full heading. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGrid() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null, syncActivity = {})
}

/** Scrolled down the grid: the heading in the slim bar. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGridScrolled() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 6, openMenu = null, syncActivity = {})
}

/** Grouped by artist at the top of the grid: each artist's header over its albums. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGrouped() {
    LibraryGridScene(grid = SceneGrid.GROUPED, firstItem = 0, openMenu = null, syncActivity = {})
}

/** Grouped, scrolled into the third artist's albums: its header pinned under the slim bar. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGroupedPinned() {
    LibraryGridScene(grid = SceneGrid.GROUPED, firstItem = 10, openMenu = null, syncActivity = {})
}

/** The ⋮ menu open over the albums view. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryMoreMenu() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = LibraryHeaderMenu.MORE, syncActivity = {})
}

/** The heading's menu of views open. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryViewMenu() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = LibraryHeaderMenu.VIEWS, syncActivity = {})
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun AlbumDetail() {
    AlbumDetailScene()
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_NO)
@Composable
fun LibraryGridLight() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null, syncActivity = {})
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_NO)
@Composable
fun AlbumDetailLight() {
    AlbumDetailScene()
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightNeutral() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.NEUTRAL, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightSlate() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.SLATE, AccentChoice.TEAL)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightPlum() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.PLUM, AccentChoice.PURPLE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkNeutral() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.NEUTRAL, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkSlate() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.SLATE, AccentChoice.TEAL)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkPlum() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.PLUM, AccentChoice.PURPLE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightMidnight() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.MIDNIGHT, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkMidnight() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.MIDNIGHT, AccentChoice.BLUE)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightForest() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.FOREST, AccentChoice.GREEN)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkForest() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.FOREST, AccentChoice.GREEN)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceLightSand() {
    AppearanceScene(AppearanceMode.LIGHT, SurfaceTone.SAND, AccentChoice.AMBER)
}

@Preview(device = PHONE_SPEC)
@Composable
fun AppearanceDarkSand() {
    AppearanceScene(AppearanceMode.DARK, SurfaceTone.SAND, AccentChoice.AMBER)
}

@Composable
private fun AppearanceScene(
    mode: AppearanceMode,
    tone: SurfaceTone,
    accent: AccentChoice,
) {
    val store = remember(mode, tone, accent) { AppearanceStore(AppearancePreferences(mode, accent, tone)) {} }
    BaeTheme(appearance = store) {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            // The section's coroutine scope reports bugs; a scene has nothing to report to.
            CompositionLocalProvider(LocalBugReports provides BugReports()) {
                AppearanceSection()
            }
        }
    }
}

/** Set up sync's Dropbox folder choice, a folder chosen and one setup found holding other files. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun FolderChoiceDropbox() {
    // The choice's coroutine scope reports bugs; a scene has nothing to report to.
    CompositionLocalProvider(LocalBugReports provides BugReports()) {
        FolderChoiceScene(dropbox = true)
    }
}

/** Set up sync's OneDrive folder choice, where the person is and a folder chosen. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun FolderChoiceOneDrive() {
    CompositionLocalProvider(LocalBugReports provides BugReports()) {
        FolderChoiceScene(dropbox = false)
    }
}

/** Set up sync's S3 form after a failed region check: the reason under Region. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun S3SetupRegionMismatch() {
    S3SetupFormScene()
}

@Preview(device = PHONE_SPEC)
@Composable
fun RestoreCodeMasked() {
    RestoreCodeShot(shown = false)
}

@Preview(device = PHONE_SPEC)
@Composable
fun RestoreCodeShown() {
    RestoreCodeShot(shown = true)
}

@Composable
private fun RestoreCodeShot(shown: Boolean) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            CompositionLocalProvider(LocalBugReports provides BugReports()) {
                RestoreCodeScene(shown)
            }
        }
    }
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES, fontScale = 1.5f)
@Composable
fun SyncUpdateRequired() {
    BaeTheme {
        AndroidErrorBanner(error = DisplayError(LocalContext.current.coreString("core.error.sync_update_required")))
    }
}

/** The Sync page's status block: nothing wrong. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SyncStatusSynced() {
    SyncStatusScene(syncShown(BridgeSyncStatusWord.SYNCED, "core.sync.synced", BridgeSyncTone.SUCCESS))
}

/** The Sync page's status block: sync held back, no problem. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SyncStatusWaiting() {
    SyncStatusScene(syncShown(BridgeSyncStatusWord.WAITING_TO_SYNC, "core.sync.status.waiting_to_sync", BridgeSyncTone.NEUTRAL))
}

/** The Sync page's status block: the storage can't be reached, and when sync last finished. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SyncStatusOffline() {
    SyncStatusScene(syncShown(BridgeSyncStatusWord.OFFLINE, "core.sync.offline", BridgeSyncTone.WARNING, lastSynced = true))
}

/** The Sync page's status block: signed out of Dropbox, with its sign-in. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SyncStatusSignedOut() {
    SyncStatusScene(
        syncShown(
            BridgeSyncStatusWord.SIGNED_OUT_OF_DROPBOX,
            "core.sync.status.signed_out_dropbox",
            BridgeSyncTone.WARNING,
            fix = BridgeSyncFix.SignIn(BridgeCloudProvider.DROPBOX),
            lastSynced = true,
        ),
    )
}

/** The Sync page's status block: the last pass failed, with its cause and Retry now. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SyncStatusFailed() {
    SyncStatusScene(
        syncShown(
            BridgeSyncStatusWord.SYNC_FAILED,
            "core.sync.failed",
            BridgeSyncTone.DANGER,
            cause = BridgeSyncCause.Message(BridgeMessage("core.sync.cause.disk_failed", emptyList()), null),
            fix = BridgeSyncFix.RetryNow,
            lastSynced = true,
        ),
    )
}

internal const val TWO_HOURS_SECONDS = 7_200L

internal fun syncShown(
    status: BridgeSyncStatusWord,
    statusKey: String,
    tone: BridgeSyncTone,
    cause: BridgeSyncCause? = null,
    fix: BridgeSyncFix? = null,
    lastSynced: Boolean = false,
) = BridgeSyncShown(
    status = status,
    statusKey = statusKey,
    tone = tone,
    cause = cause,
    fix = fix,
    lastSynced = if (lastSynced) Instant.now().minusSeconds(TWO_HOURS_SECONDS) else null,
)

/** The About page. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SettingsAbout() {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column {
                ScreenTopBar(title = stringResource(R.string.settings_about), onBack = {})
                SettingsAboutSection(version = "0.0-dev")
            }
        }
    }
}

/** The Settings menu of a synced library. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SettingsMenu() {
    SettingsMenuScene(library = "Home music · Synced")
}

/** The Settings menu of a library whose storage this phone can't reach. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SettingsMenuOffline() {
    SettingsMenuScene(library = "Home music · Offline")
}

/** The Settings menu of a library whose cloud storage is full. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun SettingsMenuProblem() {
    SettingsMenuScene(library = "Home music · Cloud full")
}

@Composable
private fun SettingsMenuScene(library: String) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column {
                ScreenTopBar(title = stringResource(R.string.settings), onBack = {})
                SettingsMenu(
                    summaries =
                        SettingsSummaries(
                            appearance = "System · Midnight · Pink",
                            library = library,
                            playback = "Pauses between sides · Restores on launch",
                            transfers = "3 pins at once",
                            casting = "Off",
                            about = "Version 0.0-dev",
                        ),
                    onOpen = {},
                    onPush = {},
                )
            }
        }
    }
}

/** The Library page of a synced library, for an admin. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibrarySettings() {
    LibrarySettingsScene(sync = SyncRowSummary("Synced", StatusTone.SUCCESS), hasStorage = true)
}

/** The Library page of a library whose sync failed. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibrarySettingsProblem() {
    LibrarySettingsScene(sync = SyncRowSummary("Sync failed", StatusTone.DANGER), hasStorage = true)
}

/** The Library page of a library that is only on this phone. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibrarySettingsNotSynced() {
    LibrarySettingsScene(sync = SyncRowSummary("Not synced", null), hasStorage = false)
}

@Composable
private fun LibrarySettingsScene(
    sync: SyncRowSummary,
    hasStorage: Boolean,
) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            LibrarySettingsContent(
                name = "Library Name",
                canRename = true,
                sync = sync,
                peopleAndDevices = "2 people · 3 devices",
                hasStorage = hasStorage,
                onBack = {},
                onRename = {},
                onOpenSync = {},
                onOpenPeople = {},
                onOpenRestoreCode = {},
            )
        }
    }
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun MiniPlayerWaitingOnStorage() {
    MiniPlayerScene(PlayerSceneState.WAITING_ON_STORAGE)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun MiniPlayerPlayingWithAGap() {
    MiniPlayerScene(PlayerSceneState.PLAYING_WITH_A_GAP)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun MiniPlayerAllHere() {
    MiniPlayerScene(PlayerSceneState.ALL_HERE)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun FullPlayerWaitingOnStorage() {
    FullPlayerScene(PlayerSceneState.WAITING_ON_STORAGE)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun FullPlayerPlayingWithAGap() {
    FullPlayerScene(PlayerSceneState.PLAYING_WITH_A_GAP)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun FullPlayerAllHere() {
    FullPlayerScene(PlayerSceneState.ALL_HERE)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun Transfers() {
    TransfersScene(wifiOnly = false)
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun TransfersWifiOnly() {
    TransfersScene(wifiOnly = true)
}

@Composable
private fun TransfersScene(wifiOnly: Boolean) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            TransfersContent(
                unmeteredOnly = LiveValue.Current(wifiOnly),
                concurrency = 3u,
                limit = 8u,
                onBack = {},
                onSetUnmeteredOnly = {},
                onSelectConcurrency = {},
            )
        }
    }
}

/** People and devices as an admin sees it: You, and People with a row that opens. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun PeopleAndDevices() {
    PeopleScene(PeoplePage.People)
}

/** The same page for a member, whose person rows open nothing. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun PeopleAndDevicesMember() {
    PeopleScene(PeoplePage.People, asAdmin = false)
}

/** Another person's page: their role, removing them, their devices. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun PeopleAndDevicesPerson() {
    PeopleScene(sceneOtherPerson)
}

/** This person's own devices, this one reading This device, with Add device last. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun PeopleAndDevicesYours() {
    PeopleScene(PeoplePage.Devices)
}

/** An error notice's looks: filled with Details and Retry, titled with advice, quiet, and quiet with the ×. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun NoticeLooks() {
    val error = DisplayError("Couldn’t reach the cloud.", "dns error: failed to lookup address information")
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(
                modifier = Modifier.padding(ThemeSpace.edge),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
            ) {
                AndroidNotice(error, action = ErrorAction("Retry") {})
                AndroidNotice(
                    title = "bae couldn’t open your library",
                    advice = "Check your connection, then quit and reopen bae.",
                    error = error,
                )
                AndroidNotice(
                    title = "Couldn't pin",
                    advice = null,
                    error = error,
                    action = ErrorAction("Retry") {},
                    look = NoticeLook.QUIET,
                )
                AndroidNotice(error, action = ErrorAction("Retry") {}, look = NoticeLook.QUIET, onDismiss = {})
                AndroidNotice(
                    title = "Couldn’t read your library",
                    advice = null,
                    error = error,
                    action = ErrorAction("Retry") {},
                    look = NoticeLook.CENTERED,
                    modifier = Modifier.height(300.dp),
                )
            }
        }
    }
}

/** An album page's pin control in each state, then its upload line queued and with a file's original missing. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun AlbumPinStates() {
    val downloads = PreviewData.downloadStore(downloadingReleaseId = "rel-1")
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(
                modifier = Modifier.padding(ThemeSpace.edge),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
            ) {
                listOf(
                    BridgeReleaseDownloadStatus.Available,
                    BridgeReleaseDownloadStatus.Queued,
                    BridgeReleaseDownloadStatus.Downloading,
                    BridgeReleaseDownloadStatus.Failed("Network unreachable"),
                    BridgeReleaseDownloadStatus.Downloaded,
                ).forEach { status ->
                    AndroidDownloadControl(
                        status = status,
                        releaseId = "rel-1",
                        downloads = downloads,
                        unpinning = false,
                        onDownload = {},
                        onCancel = {},
                        onRetry = {},
                        onRemove = {},
                    )
                }
                val queued = LocalContext.current.coreString("core.outbox.phase.queued")
                AndroidUploadLine(queued, null)
                AndroidUploadLine(queued, "01 Track Title.flac")
            }
        }
    }
}

/** The Pinning screen's list: an active and two queued pins under "1 pinning · 2 queued to pin", then a paused queue's. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun PinningList() {
    val downloads = PreviewData.downloadStore(downloadingReleaseId = "rel-1")
    val context = LocalContext.current
    // The summary lines as the screen joins them, from core's words without a call into the bridge.
    val running =
        context.countLabelsText(
            listOf(
                BridgeCountLabel(key = "core.queue.pinning", count = 1u),
                BridgeCountLabel(key = "core.queue.queued_to_pin", count = 2u),
            ),
        )
    val paused =
        listOf(
            context.countLabelsText(listOf(BridgeCountLabel(key = "core.queue.queued_to_pin", count = 2u))),
            context.coreString("core.queue.wait.paused"),
        ).joinToString(" · ")
    val ops =
        listOf(
            PreviewData.downloadOp(releaseId = "rel-1", state = BridgeDownloadState.Active),
            PreviewData.downloadOp(releaseId = "rel-2", state = BridgeDownloadState.Queued),
            PreviewData.downloadOp(releaseId = "rel-3", state = BridgeDownloadState.Queued),
        )
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column {
                listOf(running to ops, paused to ops.drop(1)).forEach { (summary, rows) ->
                    Box(modifier = Modifier.weight(1f)) {
                        DownloadsList(
                            summary = summary,
                            count = rows.size,
                            row = rows::getOrNull,
                            downloads = downloads,
                            listState = rememberLazyListState(),
                            onCancel = {},
                            onRetry = {},
                        )
                    }
                    HorizontalDivider()
                }
            }
        }
    }
}

/** The downloads list's entries: queued, running, failed, and one whose release is no longer in the library. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun DownloadEntries() {
    val downloads = PreviewData.downloadStore(downloadingReleaseId = "rel-2")
    val ops =
        listOf(
            PreviewData.downloadOp(releaseId = "rel-1", state = BridgeDownloadState.Queued),
            PreviewData.downloadOp(releaseId = "rel-2", state = BridgeDownloadState.Active),
            PreviewData.downloadOp(releaseId = "rel-3", state = BridgeDownloadState.Failed("Network unreachable")),
            PreviewData.downloadOp(releaseId = "rel-4", state = BridgeDownloadState.Queued).copy(release = null),
        )
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column {
                ops.forEach { op ->
                    AndroidDownloadRow(
                        op = op,
                        progress = downloads.latestProgress(op.releaseId),
                        onCancel = {},
                        onRetry = {},
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}
