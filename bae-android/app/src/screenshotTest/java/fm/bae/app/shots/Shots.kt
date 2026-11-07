package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.LocalBugReports
import fm.bae.app.coreString
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AlbumDetailScene
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.AppearanceStore
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.library.ErrorBanner
import fm.bae.app.ui.library.LibraryGridScene
import fm.bae.app.ui.library.LibraryHeaderMenu
import fm.bae.app.ui.library.SceneGrid
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeScene
import fm.bae.app.ui.playback.FullPlayerScene
import fm.bae.app.ui.playback.MiniPlayerScene
import fm.bae.app.ui.playback.PlayerSceneState
import fm.bae.app.ui.settings.AppearanceSection
import fm.bae.app.ui.settings.CastingPage
import fm.bae.app.ui.settings.RestoreCodeScene
import fm.bae.app.ui.settings.S3SetupFormScene
import fm.bae.app.ui.settings.TransfersContent
import fm.bae.app.ui.sync.SyncActivityList
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeRendererKind

// Screenshot scenes captured by scripts/shots/android.sh. Each function is one
// scene: the plugin renders it to a PNG named after the function, and the script
// copies it to <scene-id>@android.png. Every scene renders a shared, session-free
// composition from the production UI (defined in main next to its @Preview) in
// the app chrome — never a capture-only re-layout. Rendered at a current phone
// size, with additional light and tinted appearance captures. Function name ⇒ scene id:
//   Welcome ⇒ welcome, WelcomeLibraries ⇒ welcome-libraries, LibraryGrid ⇒ library-grid, LibraryGridScrolled ⇒
//   library-grid-scrolled, LibraryGrouped ⇒ library-grouped, LibraryGroupedPinned ⇒
//   library-grouped-pinned, LibraryMoreMenu ⇒ library-more-menu, LibraryViewMenu ⇒
//   library-view-menu, AlbumDetail ⇒ album-detail, CastingSettingsFound ⇒
//   settings-casting, CastingSettingsLooking ⇒ settings-casting-looking.

private const val PHONE_SPEC = "spec:width=411dp,height=914dp,dpi=420"

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

/** The library home at the top of the list: the full heading. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGrid() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null)
}

/** Scrolled down the grid: the heading in the slim bar. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGridScrolled() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 6, openMenu = null)
}

/** Grouped by artist at the top of the grid: each artist's header over its albums. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGrouped() {
    LibraryGridScene(grid = SceneGrid.GROUPED, firstItem = 0, openMenu = null)
}

/** Grouped, scrolled into the third artist's albums: its header pinned under the slim bar. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryGroupedPinned() {
    LibraryGridScene(grid = SceneGrid.GROUPED, firstItem = 10, openMenu = null)
}

/** The ⋮ menu open over the albums view. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryMoreMenu() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = LibraryHeaderMenu.MORE)
}

/** The heading's menu of views open. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun LibraryViewMenu() {
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = LibraryHeaderMenu.VIEWS)
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
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null)
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
fun SyncActivityNeedingAttention() {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            SyncActivityList(
                activity = PreviewData.syncActivity(needsAttention = true),
                uploadTransfer = PreviewData.uploadTransferSource(),
                error = null,
                onOpenSettings = {},
                onOpenDownloads = {},
                onRetryNow = {},
            )
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

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingSettingsFound() {
    CastingSettingsScene(
        devices =
            listOf(
                BridgeCastDevice(id = "cast-den", name = "Den receiver", kind = BridgeRendererKind.DLNA),
                BridgeCastDevice(id = "cast-kitchen", name = "Kitchen", kind = BridgeRendererKind.AIR_PLAY),
                BridgeCastDevice(id = "cast-living-room", name = "Living Room", kind = BridgeRendererKind.CAST),
            ),
    )
}

@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun CastingSettingsLooking() {
    CastingSettingsScene(devices = emptyList())
}

/** The Casting page with casting on and [devices] found. */
@Composable
private fun CastingSettingsScene(devices: List<BridgeCastDevice>) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            CastingPage(
                enabled = true,
                devices = devices,
                transcodeFormat = BridgeCastTranscodeFormat.MP3,
                onBack = {},
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
