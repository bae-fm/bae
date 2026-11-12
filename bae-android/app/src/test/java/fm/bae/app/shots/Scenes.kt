package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidSheetBody
import fm.bae.app.ui.components.DialogHead
import fm.bae.app.ui.components.DialogParts
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone
import java.time.Duration
import java.time.Instant

// What the shots share: the surfaces a screen, a sheet and a dialog are drawn on, and the data
// they are staged with.

/**
 * A titled sheet's body on the sheet's surface: the sheet's own window isn't drawn in a test. With [behind], the sheet
 * rises from the foot of that screen, which the scrim dims, as a bottom sheet opens over its page.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SheetScene(
    title: String?,
    behind: (@Composable () -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    if (behind == null) {
        ShotTheme {
            Surface(color = BottomSheetDefaults.ContainerColor) {
                SheetWithHandle(title, content)
            }
        }
    } else {
        ScreenScene {
            Box(modifier = Modifier.fillMaxSize()) {
                behind()
                Box(
                    modifier = Modifier.fillMaxSize().background(MaterialTheme.colorScheme.scrim.copy(alpha = SCRIM_ALPHA)),
                    contentAlignment = Alignment.BottomCenter,
                ) {
                    Surface(
                        modifier = Modifier.fillMaxWidth(),
                        shape = BottomSheetDefaults.ExpandedShape,
                        color = BottomSheetDefaults.ContainerColor,
                    ) {
                        SheetWithHandle(title, content)
                    }
                }
            }
        }
    }
}

/** The sheet's body under the drag handle Material's modal sheet draws above it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SheetWithHandle(
    title: String?,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        BottomSheetDefaults.DragHandle()
        Column(modifier = Modifier.fillMaxWidth()) { AndroidSheetBody(title, content) }
    }
}

/** A component on the page colour, padded as a preview cell is; [wide] makes it as wide as the phone. */
@Composable
internal fun ComponentScene(
    wide: Boolean,
    content: @Composable () -> Unit,
) {
    ShotTheme {
        Surface(color = MaterialTheme.colorScheme.background) {
            Box(modifier = (if (wide) Modifier.width(PHONE_WIDTH) else Modifier).padding(ThemeSpace.edge)) { content() }
        }
    }
}

private val PHONE_WIDTH = 411.dp

/** A screen of its own, on the screen's background, in the look [appearance] gives (the boards' by default). */
@Composable
internal fun ScreenScene(
    appearance: AppearancePreferences = BOARD_APPEARANCE,
    content: @Composable () -> Unit,
) {
    ShotTheme(appearance) {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { content() }
    }
}

/**
 * The look of a phone whose person chose nothing: the system's mode, Midnight and Pink, so Appearance
 * reads System. The shots run at night, so the system's mode is dark.
 */
internal val SYSTEM_APPEARANCE =
    AppearancePreferences(mode = AppearanceMode.SYSTEM, accent = AccentChoice.PINK, tone = SurfaceTone.MIDNIGHT)

/**
 * A dialog's [parts] drawn in place, as its window would show them: over [behind], dimmed by the scrim, centred on
 * the phone, with title, message, content and buttons on the dialog's surface, 340 dp wide, 16 apart with the buttons 24 below
 * the content, as the design system's dialog sets them.
 */
@Composable
internal fun InPlaceDialogScene(
    behind: @Composable () -> Unit = {},
    dialog: @Composable () -> DialogParts,
) {
    ScreenScene {
        Box(modifier = Modifier.fillMaxSize()) {
            behind()
            Box(
                modifier =
                    Modifier
                        .fillMaxSize()
                        .background(MaterialTheme.colorScheme.scrim.copy(alpha = SCRIM_ALPHA))
                        .padding(ThemeSpace.page),
                contentAlignment = Alignment.Center,
            ) {
                val parts = dialog()
                DialogSurface {
                    Column(modifier = Modifier.width(DIALOG_WIDTH), verticalArrangement = Arrangement.spacedBy(ThemeSpace.edge)) {
                        if (parts.title != null || parts.message != null) DialogHead(parts.title, parts.message)
                        parts.content?.invoke()
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.related),
                            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related, Alignment.End),
                        ) {
                            parts.dismissButton?.invoke()
                            parts.confirmButton()
                        }
                    }
                }
            }
        }
    }
}

private const val SCRIM_ALPHA = 0.32f

private val DIALOG_WIDTH = 340.dp

/** The surface a dialog's words sit on. */
@Composable
internal fun DialogSurface(content: @Composable () -> Unit) {
    Surface(
        shape = AlertDialogDefaults.shape,
        color = AlertDialogDefaults.containerColor,
        tonalElevation = AlertDialogDefaults.TonalElevation,
    ) {
        Box(modifier = Modifier.padding(ThemeSpace.section)) { content() }
    }
}

internal const val TWO_HOURS_SECONDS = 7_200L

/** How long ago the Sync boards say it last synced: two hours, or three days once signed out. */
internal val twoHoursAgo: Duration = Duration.ofSeconds(TWO_HOURS_SECONDS)
internal val threeDaysAgo: Duration = Duration.ofDays(3)

internal fun syncShown(
    status: BridgeSyncStatusWord,
    statusKey: String,
    tone: BridgeSyncTone,
    cause: BridgeSyncCause? = null,
    fix: BridgeSyncFix? = null,
    lastSyncedAgo: Duration? = null,
) = BridgeSyncShown(
    status = status,
    statusKey = statusKey,
    tone = tone,
    cause = cause,
    fix = fix,
    lastSynced = lastSyncedAgo?.let { Instant.now().minus(it) },
)

internal val sceneDropbox =
    BridgeStorageLocation.Dropbox(
        location = BridgeDropboxLocation.AppFolder,
        accountLabel = "mara.quill@example.com",
        folderLabel = BridgeFolderLabel(name = "Music", webUrl = "https://www.dropbox.com/home/Music"),
    )

internal val sceneLibrary =
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
