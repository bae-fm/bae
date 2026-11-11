package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
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
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidSheetBody
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
import java.time.Instant

// What the shots share: the surfaces a screen, a sheet and a dialog are drawn on, and the data
// they are staged with.

/** A titled sheet's body on the sheet's surface: the sheet's own window isn't drawn in a test. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SheetScene(
    title: String?,
    content: @Composable ColumnScope.() -> Unit,
) {
    BaeTheme {
        Surface(color = BottomSheetDefaults.ContainerColor) {
            AndroidSheetBody(title, content)
        }
    }
}

/** A component on the page colour, padded as a preview cell is; [wide] makes it as wide as the phone. */
@Composable
internal fun ComponentScene(
    wide: Boolean,
    content: @Composable () -> Unit,
) {
    BaeTheme {
        Surface(color = MaterialTheme.colorScheme.background) {
            Box(modifier = (if (wide) Modifier.width(PHONE_WIDTH) else Modifier).padding(ThemeSpace.edge)) { content() }
        }
    }
}

private val PHONE_WIDTH = 411.dp

/** A screen of its own, on the screen's background. */
@Composable
internal fun ScreenScene(content: @Composable () -> Unit) {
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { content() }
    }
}

/** A dialog's words on the dialog's own surface: the dialog opens a window a test doesn't draw. */
@Composable
internal fun DialogScene(content: @Composable () -> Unit) {
    BaeTheme {
        Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
            DialogSurface(content)
        }
    }
}

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
