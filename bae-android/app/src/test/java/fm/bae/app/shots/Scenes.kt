package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
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
import fm.bae.app.ui.components.AndroidSheetHeading
import fm.bae.app.ui.components.DialogHead
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.dialogPane
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

// What the shots share: the surfaces a screen, a sheet and a dialog are drawn on, and the data
// they are staged with.

/**
 * A sheet's body on the sheet's surface, rising from the foot of [behind], which the scrim dims, as a bottom
 * sheet opens over its page; [top] pins a tall one, as in [SheetOverScrim]. The sheet's own window isn't drawn in a test.
 */
@Composable
internal fun SheetScene(
    heading: AndroidSheetHeading = AndroidSheetHeading(),
    top: Dp? = null,
    behind: @Composable () -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    ScreenScene {
        Box(modifier = Modifier.fillMaxSize()) {
            behind()
            SheetOverScrim(heading, top = top, content = content)
        }
    }
}

/**
 * A sheet at the foot of the space it is in, with the scrim over the rest of that space. With [top], the
 * sheet is pinned to that height from the top of the space, as the design system's `top` pins a tall one.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SheetOverScrim(
    heading: AndroidSheetHeading = AndroidSheetHeading(),
    top: Dp? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Box(
        modifier = Modifier.fillMaxSize().background(BaeTheme.colors.scrim),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Surface(
            modifier = Modifier.fillMaxWidth().then(if (top == null) Modifier else Modifier.fillMaxHeight().padding(top = top)),
            shape = BottomSheetDefaults.ExpandedShape,
            color = BottomSheetDefaults.ContainerColor,
        ) {
            SheetWithHandle(heading, content)
        }
    }
}

/** The sheet's body under the drag handle Material's modal sheet draws above it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SheetWithHandle(
    heading: AndroidSheetHeading,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        DragHandleBar()
        Column(modifier = Modifier.fillMaxWidth()) { AndroidSheetBody(heading, content) }
    }
}

/**
 * Material's drag handle, drawn as it draws it: a 32 by 4 bar with 22 above and below it. The button TalkBack
 * names "Drag handle" is the bar, as the board's is, and not the 48 dp that the bar and its margins take.
 */
@Composable
private fun DragHandleBar() {
    Box(
        modifier =
            Modifier
                .padding(vertical = 22.dp)
                .semantics {
                    role = Role.Button
                    contentDescription = "Drag handle"
                }.size(width = 32.dp, height = 4.dp)
                .background(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f), MaterialTheme.shapes.extraLarge),
    )
}

/**
 * A component on the page colour, at the top left with no margin and at [size], its board's: a preview cell
 * holds it, and the view is proposed the cell's size.
 */
@Composable
internal fun ComponentScene(
    size: DpSize,
    content: @Composable () -> Unit,
) {
    ShotTheme {
        Surface(color = MaterialTheme.colorScheme.background) {
            Box(modifier = Modifier.size(size)) { content() }
        }
    }
}

/**
 * [content] as wide as it is, as a preview cell that hugs its content draws a component that fills its parent's
 * width on a screen: a row that stretches across a dialog is as wide as its words in the board's preview.
 */
@Composable
internal fun HuggingContent(content: @Composable () -> Unit) {
    Box(modifier = Modifier.width(IntrinsicSize.Max)) { content() }
}

/** [content] inside a screen's side padding, with [top] above it. */
@Composable
internal fun InsideScreenEdges(
    top: Dp = 0.dp,
    content: @Composable () -> Unit,
) {
    Box(modifier = Modifier.padding(start = ThemeSpace.edge, top = top, end = ThemeSpace.edge)) { content() }
}

/** The page inset around an Android component preview that has no named rows. */
@Composable
internal fun InsidePreviewPage(content: @Composable () -> Unit) {
    Box(modifier = Modifier.padding(ThemeSpace.edge)) { content() }
}

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
 * the phone, with title, message, content and buttons on the dialog's surface, as wide as it says, 16 apart with the buttons 24 below
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
                        .padding(horizontal = ThemeSpace.page),
                contentAlignment = Alignment.Center,
            ) {
                InPlaceDialog(dialog())
            }
        }
    }
}

/**
 * A dialog's [parts] on the dialog's surface, as wide as it says, 16 apart with the buttons 24 below the content, as the
 * design system's dialog sets them.
 */
@Composable
internal fun InPlaceDialog(parts: DialogParts) {
    DialogSurface(modifier = Modifier.width(parts.width).dialogPane(parts.title)) {
        // Material's dialog sets its content's text in bodyMedium, so unstyled text in [parts] draws as it does there.
        ProvideTextStyle(MaterialTheme.typography.bodyMedium) {
            Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ThemeSpace.edge)) {
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
}

/**
 * A component board's dialog: [parts] centred in the board's cell and dimmed by the scrim over the whole cell, as the
 * design system's dialog draws inside the preview cell that holds it.
 */
@Composable
internal fun DialogInCell(parts: DialogParts) {
    Box(
        modifier = Modifier.fillMaxSize().background(BaeTheme.colors.scrim),
        contentAlignment = Alignment.Center,
    ) {
        InPlaceDialog(parts)
    }
}

/** A screen with a field in focus: the keyboard's inset holds the bottom of the screen, and [content] fits above it. */
@Composable
internal fun KeyboardScene(content: @Composable () -> Unit) {
    ScreenScene { Box(modifier = Modifier.fillMaxSize().padding(bottom = KEYBOARD_HEIGHT)) { content() } }
}

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
internal fun DialogSurface(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Surface(
        modifier = modifier,
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
    lastSynced = lastSyncedAgo?.let { ShotClock.before(it) },
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

/** [content] with the artwork that the boards draw, named by its image ID. */
@Composable
internal fun Covers(content: @Composable () -> Unit) =
    CompositionLocalProvider(
        LocalImageStore provides
            ImageStore(
                fetchLibraryImageBytes = { ref ->
                    if (ref.id == "Northbound") {
                        northboundCover
                    } else {
                        val resource =
                            requireNotNull(BoardShot::class.java.getResourceAsStream("/${ref.id}.png")) {
                                "No board artwork for ${ref.id}"
                            }
                        resource.use { it.readBytes() }
                    }
                },
                dispatcher = Dispatchers.Unconfined,
            ),
        content = content,
    )

/** The board's fallback artwork for Northbound, with its green and teal endpoints. */
private val northboundCover by lazy {
    TestImages.gradientPng(size = 256, topLeft = 0xFF51B861.toInt(), bottomRight = 0xFF215957.toInt())
}
