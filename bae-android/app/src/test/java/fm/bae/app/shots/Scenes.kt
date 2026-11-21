package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.TestImages
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.AppearanceMode
import fm.bae.app.ui.appearance.AppearancePreferences
import fm.bae.app.ui.appearance.SurfaceTone
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidDialogActions
import fm.bae.app.ui.components.AndroidSheetBody
import fm.bae.app.ui.components.DialogHead
import fm.bae.app.ui.components.DialogParts
import kotlinx.coroutines.Dispatchers
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeFolderLabel
import uniffi.bae_bridge.BridgeHistoryPerson
import uniffi.bae_bridge.BridgeHistoryShown
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeListCountExtra
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
                    modifier = Modifier.fillMaxSize().background(BaeTheme.colors.scrim),
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
        BottomSheetDefaults.DragHandle(modifier = Modifier.semantics { role = Role.Button })
        Column(modifier = Modifier.fillMaxWidth()) { AndroidSheetBody(title, content) }
    }
}

/**
 * A component on the page colour, at the top left with no margin, as a preview cell holds it; [wide] makes it as wide
 * as a phone screen's margins leave a component.
 */
@Composable
internal fun ComponentScene(
    wide: Boolean,
    content: @Composable () -> Unit,
) {
    ShotTheme {
        Surface(color = MaterialTheme.colorScheme.background) {
            Box(modifier = if (wide) Modifier.width(PHONE_COLUMN) else Modifier) { content() }
        }
    }
}

/** A phone 411 dp wide, less the screen's margin on each side. */
private val PHONE_COLUMN = 411.dp - ThemeSpace.edge * 2

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
 * the content, as the design system's dialog sets them. With [keyboardUp], a field has focus: the keyboard's inset
 * holds the bottom of the screen, and the page and the dialog fit in the space above it, as the board's keyboard
 * leaves them.
 */
@Composable
internal fun InPlaceDialogScene(
    behind: @Composable () -> Unit = {},
    keyboardUp: Boolean = false,
    dialog: @Composable () -> DialogParts,
) {
    ScreenScene {
        Box(modifier = Modifier.fillMaxSize().padding(bottom = if (keyboardUp) KEYBOARD_HEIGHT else 0.dp)) {
            behind()
            Box(
                modifier =
                    Modifier
                        .fillMaxSize()
                        .background(BaeTheme.colors.scrim)
                        .padding(ThemeSpace.page),
                contentAlignment = Alignment.Center,
            ) {
                InPlaceDialog(dialog())
            }
        }
    }
}

/**
 * A dialog's [parts] on the dialog's surface, 340 dp wide, 16 apart with the buttons 24 below the content, as the
 * design system's dialog sets them.
 */
@Composable
internal fun InPlaceDialog(parts: DialogParts) {
    DialogSurface {
        Column(modifier = Modifier.width(DIALOG_WIDTH), verticalArrangement = Arrangement.spacedBy(ThemeSpace.edge)) {
            if (parts.title != null || parts.message != null) DialogHead(parts.title, parts.message)
            parts.content?.invoke()
            Box(
                modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.related),
                contentAlignment = Alignment.CenterEnd,
            ) {
                AndroidDialogActions(parts.action, parts.dismiss)
            }
        }
    }
}

/** A screen with a field in focus: the keyboard's inset holds the bottom of the screen, and [content] fits above it. */
@Composable
internal fun KeyboardScene(content: @Composable () -> Unit) {
    ScreenScene { Box(modifier = Modifier.fillMaxSize().padding(bottom = KEYBOARD_HEIGHT)) { content() } }
}

private val DIALOG_WIDTH = 340.dp

/** What the count of History says about whose plays it shows: [shown], and the library's two other people to choose. */
internal fun historyChoice(shown: BridgeHistoryShown) =
    BridgeListCountExtra.History(
        people =
            listOf(
                BridgeHistoryPerson(person = "ana", name = "Ana", chosen = false),
                BridgeHistoryPerson(person = "ben", name = "Ben", chosen = shown is BridgeHistoryShown.Person),
            ),
        shown = shown,
    )

/**
 * The height of the board's keyboard (the design system's `AndroidKeyboard`): 10 above four rows of 46-high keys, 10
 * apart, then 10 more to the 28-high gesture bar.
 */
private val KEYBOARD_HEIGHT = 262.dp

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
    shortStatusKey = statusKey,
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

/**
 * An open menu's [items] on the menu's surface, [end] in from the box's right edge and [top] down from its top:
 * layoutlib draws an open menu's popup blank, so the menu's contents are drawn in a surface like its own. A menu is a
 * window of its own, which TalkBack reads after the page it opens over, so a scene draws it after the page in the
 * same box.
 */
@Composable
internal fun BoxScope.OpenMenu(
    top: Dp,
    end: Dp = 0.dp,
    items: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        shape = MenuDefaults.shape,
        color = MenuDefaults.containerColor,
        tonalElevation = MenuDefaults.TonalElevation,
        shadowElevation = MenuDefaults.ShadowElevation,
        modifier = Modifier.align(Alignment.TopEnd).padding(end = end).offset(y = top),
    ) {
        Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(IntrinsicSize.Max), content = items)
    }
}

/** A component on the backdrop colour, as the lightbox draws its controls. */
@Composable
internal fun OnTheBackdrop(content: @Composable () -> Unit) = Row(modifier = Modifier.background(BaeTheme.colors.backdrop)) { content() }

/** [content] with an image store that resolves no image, so each cover draws its placeholder. */
@Composable
internal fun Images(content: @Composable () -> Unit) =
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved(), content = content)

/** [content] with an image store that draws a picture for every cover, a different one for each image's id. */
@Composable
internal fun Covers(content: @Composable () -> Unit) =
    CompositionLocalProvider(
        LocalImageStore provides
            ImageStore(
                fetchLibraryImageBytes = { TestImages.png(COVER_PIXELS, seed = it.id.hashCode()) },
                dispatcher = Dispatchers.Unconfined,
            ),
        content = content,
    )

/** The width of a test cover's picture, as wide as the largest cover a preview draws at two pixels a point. */
private const val COVER_PIXELS = 240
