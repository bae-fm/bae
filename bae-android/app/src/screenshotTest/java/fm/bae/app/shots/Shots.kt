package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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
import fm.bae.app.ui.albumdetail.DeleteAlbumDialog
import fm.bae.app.ui.albumdetail.DownloadControlBody
import fm.bae.app.ui.albumdetail.UploadLineBody
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ContentSheetBody
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.RemoveLibraryText
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.TitledErrorNotice
import fm.bae.app.ui.components.TitledSheetContent
import fm.bae.app.ui.downloads.DownloadQueueRow
import fm.bae.app.ui.downloads.DownloadsList
import fm.bae.app.ui.library.ErrorBanner
import fm.bae.app.ui.library.LibraryGridScene
import fm.bae.app.ui.library.LibraryHeaderMenu
import fm.bae.app.ui.library.SceneGrid
import fm.bae.app.ui.onboarding.LibraryDetailsRows
import fm.bae.app.ui.onboarding.RemovalFailedText
import fm.bae.app.ui.onboarding.RestoreText
import fm.bae.app.ui.onboarding.S3KeyDialog
import fm.bae.app.ui.onboarding.S3KeyInput
import fm.bae.app.ui.onboarding.StandingRead
import fm.bae.app.ui.onboarding.WelcomeActions
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeScene
import fm.bae.app.ui.playback.CastReceiverRows
import fm.bae.app.ui.playback.FullPlayerScene
import fm.bae.app.ui.playback.MiniPlayerScene
import fm.bae.app.ui.playback.PlayerSceneState
import fm.bae.app.ui.settings.AppearanceSection
import fm.bae.app.ui.settings.CastingPage
import fm.bae.app.ui.settings.FolderChoiceScene
import fm.bae.app.ui.settings.LibrarySettingsContent
import fm.bae.app.ui.settings.PeoplePage
import fm.bae.app.ui.settings.PeopleScene
import fm.bae.app.ui.settings.RestoreCodeScene
import fm.bae.app.ui.settings.S3SetupFormScene
import fm.bae.app.ui.settings.SyncRowSummary
import fm.bae.app.ui.settings.SyncStatusScene
import fm.bae.app.ui.settings.TransfersContent
import fm.bae.app.ui.settings.sceneOtherPerson
import fm.bae.app.ui.signInSettingsProblem
import fm.bae.app.ui.sync.ActivityList
import fm.bae.app.ui.sync.ActivityWords
import fm.bae.app.ui.sync.activityProblems
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat
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
import uniffi.bae_bridge.BridgeRendererKind
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
// copies it to <scene-id>@android.png. Every scene renders a shared, session-free
// composition from the production UI (defined in main next to its @Preview) in
// the app chrome — never a capture-only re-layout. Rendered at a current phone
// size, with additional light and tinted appearance captures. Function name ⇒ scene id:
//   Welcome ⇒ welcome, WelcomeLibraries ⇒ welcome-libraries, WelcomeLibraryDetails ⇒ welcome-library-details,
//   WelcomeLibraryDetailsUnsent ⇒ welcome-library-details-unsent, WelcomeAddLibrary ⇒ welcome-add-library,
//   RemoveLibraryUnsent ⇒ remove-library-unsent, RemoveLibrary ⇒ remove-library,
//   RemoveLibraryFailed ⇒ remove-library-failed,
//   LibraryGrid ⇒ library-grid, LibraryGridScrolled ⇒ library-grid-scrolled, LibraryGrouped ⇒ library-grouped, LibraryGroupedPinned ⇒
//   library-grouped-pinned, LibraryMoreMenu ⇒ library-more-menu, LibraryViewMenu ⇒
//   library-view-menu, AlbumDetail ⇒ album-detail, CastPicker ⇒ cast-picker, CastingSettingsFound ⇒
//   settings-casting, CastingSettingsLooking ⇒ settings-casting-looking, LibrarySettings ⇒ settings-library,
//   LibrarySettingsProblem ⇒ settings-library-problem, LibrarySettingsNotSynced ⇒ settings-library-not-synced,
//   PeopleAndDevices ⇒ people-and-devices, PeopleAndDevicesMember ⇒ people-and-devices-member,
//   PeopleAndDevicesPerson ⇒ people-and-devices-person, PeopleAndDevicesYours ⇒ people-and-devices-yours,
//   SyncStatusSynced/Waiting/Offline/SignedOut/Failed ⇒ sync-status-synced, -waiting, -offline, -signed-out, -failed.

internal const val PHONE_SPEC = "spec:width=411dp,height=914dp,dpi=420"

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun Welcome() {
    WelcomeScene()
}

/** The welcome on a phone that holds libraries, one of them never synced. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun WelcomeLibraries() {
    WelcomeLibrariesScene()
}

/** A library's details sheet, over a Dropbox library that synced two hours ago. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun WelcomeLibraryDetails() {
    LibraryDetailsScene(
        BridgeStandingState.Sync(syncShown(BridgeSyncStatusWord.SYNCED, "core.sync.synced", BridgeSyncTone.SUCCESS)),
    )
}

/** A library's details sheet, over a library holding changes it hasn't uploaded. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun WelcomeLibraryDetailsUnsent() {
    LibraryDetailsScene(BridgeStandingState.UnsentChanges)
}

/** The Add library sheet over the welcome's list: Scan code and Enter code. */
@OptIn(ExperimentalMaterial3Api::class)
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun WelcomeAddLibrary() {
    TitledSheetShot(stringResource(R.string.welcome_add_library)) {
        WelcomeActions(onScan = {}, onEnter = {})
    }
}

/** A titled sheet's body on the sheet's surface: the sheet's own window isn't rendered by screenshot tests. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TitledSheetShot(
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    BaeTheme {
        Surface(color = BottomSheetDefaults.ContainerColor) {
            ContentSheetBody { TitledSheetContent(title, content) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LibraryDetailsScene(state: BridgeStandingState) {
    TitledSheetShot("Home music") {
        LibraryDetailsRows(
            storage =
                BridgeStorageLocation.Dropbox(
                    location = BridgeDropboxLocation.AppFolder,
                    accountLabel = "mara.quill@example.com",
                    folderLabel = BridgeFolderLabel(name = "Music", webUrl = "https://www.dropbox.com/home/Music"),
                ),
            standing =
                StandingRead.Loaded(
                    BridgeLibraryStanding(state, Instant.now().minusSeconds(TWO_HOURS_SECONDS)),
                ),
        )
    }
}

/** The confirmation to remove a library that holds changes it hasn't sent. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun RemoveLibraryUnsent() {
    DialogScene { RemoveLibraryText(sceneLibrary, RemovalLoss.Counted(holdsUnsent = true)) }
}

/** The confirmation to remove a library with nothing unsent. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun RemoveLibrary() {
    DialogScene { RemoveLibraryText(sceneLibrary, RemovalLoss.Counted(holdsUnsent = false)) }
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

/** The dialog asking for the S3 access key a restore code lacks. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun S3KeyDialogScene() {
    BaeTheme {
        S3KeyDialog(
            text = RestoreText.ADD,
            keys = S3KeyInput(accessKeyId = "AKIA4EXAMPLE"),
            error = null,
            onRestore = {},
            onBack = {},
        )
    }
}

/** The dialog for a removal that failed. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun RemoveLibraryFailed() {
    DialogScene { RemovalFailedText(sceneLibrary, DisplayError("Couldn’t reach your storage.")) }
}

private val sceneLibrary =
    BridgeLibrary(
        id = "home",
        name = "Home music",
        path = "/libraries/home",
        state =
            BridgeLibraryState.Openable(
                BridgeStorageLocation.Dropbox(
                    location = BridgeDropboxLocation.AppFolder,
                    accountLabel = "mara.quill@example.com",
                    folderLabel = null,
                ),
            ),
        isActive = false,
    )

/** A dialog's words on the dialog's own surface: the dialog opens a window the screenshot tests don't render. */
@Composable
private fun DialogScene(content: @Composable () -> Unit) {
    BaeTheme {
        Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
            Surface(
                shape = AlertDialogDefaults.shape,
                color = AlertDialogDefaults.containerColor,
                tonalElevation = AlertDialogDefaults.TonalElevation,
            ) {
                Box(modifier = Modifier.padding(ThemeSpace.section)) { content() }
            }
        }
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
fun WelcomeLight() {
    WelcomeScene()
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
        ErrorBanner(error = DisplayError(LocalContext.current.coreString("core.error.sync_update_required")))
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
            cause = BridgeSyncCause(BridgeMessage("core.sync.cause.disk_failed", emptyList()), null),
            fix = BridgeSyncFix.RetryNow,
            lastSynced = true,
        ),
    )
}

private const val TWO_HOURS_SECONDS = 7_200L

private fun syncShown(
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
fun CastingSettingsFound() {
    CastingSettingsScene(receivers = castReceivers())
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastPicker() {
    CastPickerScene(castReceivers())
}

/** The cast picker over receivers whose names are longer than a row: two lines, then an ellipsis. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastPickerLongNames() {
    CastPickerScene(
        castReceivers(
            casting = "Upstairs Living Room Sonos Arc Soundbar",
            dlna = "Denon AVR-X3800H Home Theater Receiver (Basement)",
        ),
    )
}

@Composable
private fun CastPickerScene(receivers: BridgeCastReceiverList) {
    TitledSheetShot(stringResource(R.string.cast)) {
        CastReceiverRows(
            receivers = receivers,
            connectingName = null,
            onCast = {},
            onDisconnect = {},
            looking = stringResource(R.string.cast_looking_for_devices),
        )
    }
}

/** Living Room cast to, as core lists it, then the rest by name. */
private fun castReceivers(
    casting: String = "Living Room",
    dlna: String = "Den receiver",
) = BridgeCastReceiverList(
    casting = BridgeCastReceiver(id = "cast-living-room", name = casting, kind = BridgeRendererKind.CAST),
    found =
        listOf(
            BridgeCastReceiver(id = "cast-den", name = dlna, kind = BridgeRendererKind.DLNA),
            BridgeCastReceiver(id = "cast-kitchen", name = "Kitchen", kind = BridgeRendererKind.AIR_PLAY),
        ),
)

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingSettingsLooking() {
    CastingSettingsScene(receivers = BridgeCastReceiverList(casting = null, found = emptyList()))
}

/** The Casting page with casting on and [receivers] found. */
@Composable
private fun CastingSettingsScene(receivers: BridgeCastReceiverList) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            CastingPage(
                enabled = true,
                receivers = receivers,
                connectingName = null,
                transcodeFormat = BridgeCastTranscodeFormat.MP3,
                onBack = {},
                onCast = {},
                onDisconnect = {},
                onEnabledChange = {},
                onTranscodeFormatChange = {},
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
    val error = DisplayError("Couldn’t reach your storage.", "dns error: failed to lookup address information")
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(
                modifier = Modifier.padding(ThemeSpace.edge),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
            ) {
                ErrorNotice(error, action = ErrorAction("Retry") {})
                TitledErrorNotice(
                    title = "bae couldn’t open your library",
                    advice = "Check your connection, then quit and reopen bae.",
                    error = error,
                )
                TitledErrorNotice(
                    title = "Couldn't pin",
                    advice = null,
                    error = error,
                    action = ErrorAction("Retry") {},
                    look = NoticeLook.QUIET,
                )
                ErrorNotice(error, action = ErrorAction("Retry") {}, look = NoticeLook.QUIET, onDismiss = {})
                TitledErrorNotice(
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
                    DownloadControlBody(
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
                UploadLineBody(queued, null)
                UploadLineBody(queued, "01 Track Title.flac")
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
                    DownloadQueueRow(
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
