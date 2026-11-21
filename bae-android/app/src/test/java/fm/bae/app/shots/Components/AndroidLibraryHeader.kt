package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import fm.bae.app.data.ImageContent
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.AvatarFace
import fm.bae.app.ui.library.AndroidLibraryHeader
import fm.bae.app.ui.library.LibraryBrowserState
import fm.bae.app.ui.library.PersonButton
import org.junit.Test
import uniffi.bae_bridge.BridgeSyncActivity

/** The component shots of `AndroidLibraryHeader`, one for each row of its preview. */
class AndroidLibraryHeaderShots : BoardShot() {
    @Test
    fun androidComponentLibraryHeaderTop() = component("android.component.library-header.top", wide = true) { LibraryHeader() }

    @Test
    fun androidComponentLibraryHeaderScrolled() =
        component("android.component.library-header.scrolled", wide = true) { LibraryHeader(collapse = 1f) }

    @Test
    fun androidComponentLibraryHeaderMoving() =
        component("android.component.library-header.moving", wide = true) {
            LibraryHeader(activity = PreviewData.syncActivity(needsAttention = false))
        }

    @Test
    fun androidComponentLibraryHeaderAttention() =
        component("android.component.library-header.attention", wide = true) {
            LibraryHeader(activity = idleActivity.copy(failure = PreviewData.syncActivity(needsAttention = true).failure))
        }

    @Test
    fun androidComponentLibraryHeaderPaused() =
        component("android.component.library-header.paused", wide = true) { LibraryHeader(activity = idleActivity.copy(held = true)) }

    @Test
    fun androidComponentLibraryHeaderPicture() =
        component("android.component.library-header.picture", wide = true) {
            LibraryHeader(face = AvatarFace(initials = "A", picture = ImageContent.Avatar(person = "ana", version = "1")))
        }
}

/**
 * The library's header on Albums, [collapse] of the way to its slim bar, with the Activity button over [activity] and Ana's
 * [face] at its end, which opens her menu.
 */
@Composable
private fun LibraryHeader(
    collapse: Float = 0f,
    activity: BridgeSyncActivity = idleActivity,
    face: AvatarFace = AvatarFace(initials = "A", picture = null),
) {
    val state = remember { LibraryBrowserState() }
    CompositionLocalProvider(LocalImageStore provides avatarStore()) {
        AndroidLibraryHeader(
            state = state,
            collapse = { collapse },
            onShuffleLibrary = {},
            syncActivity = { ActivityButton(activity) },
            person = {
                PersonButton(
                    state = state,
                    face = face,
                    isAdmin = true,
                    onAddDevice = {},
                    onInvite = {},
                    onPlaylists = {},
                    onHistory = {},
                    onSettings = {},
                )
            },
        )
    }
}
