package fm.bae.app.playback

import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Looper
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import fm.bae.app.data.Library
import fm.bae.app.testCoverRef
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

private const val CAR_PACKAGE = "com.example.car"
private const val WATCH_PACKAGE = "com.example.watch"
private const val PREFIX_READ =
    Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION

/**
 * A controller in another app opens the cover URIs it is handed itself, and
 * the artwork provider isn't exported, so the session must grant the
 * controller's package read access to them.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ArtworkReadGrantsTest {
    private val uriGrants = RecordingUriGrants(RuntimeEnvironment.getApplication())
    private val covers = ArtworkContentProvider.coversUri(uriGrants)

    @Test
    fun aConnectedControllerCanReadEveryCoverUntilItDisconnects() {
        val callback = callback(ArtworkReadGrants(uriGrants))
        val session = session(callback)
        val car = controller(CAR_PACKAGE, pid = 1)

        val result = callback.onConnect(session, car)

        assertTrue(result.isAccepted)
        assertEquals(listOf(UriGrant.Granted(CAR_PACKAGE, covers, PREFIX_READ)), uriGrants.events)
        val cover = ArtworkContentProvider.uriFor(uriGrants, testCoverRef("cover-1"))
        assertEquals(covers.authority, cover.authority)
        assertEquals(covers.pathSegments, cover.pathSegments.take(covers.pathSegments.size))

        callback.onDisconnected(session, car)

        assertEquals(
            listOf(
                UriGrant.Granted(CAR_PACKAGE, covers, PREFIX_READ),
                UriGrant.Revoked(CAR_PACKAGE, covers, Intent.FLAG_GRANT_READ_URI_PERMISSION),
            ),
            uriGrants.events,
        )
        session.release()
    }

    @Test
    fun aPackageKeepsAccessWhileAnyOfItsControllersIsConnected() {
        val grants = ArtworkReadGrants(uriGrants)
        val browser = controller(CAR_PACKAGE, pid = 1)
        val transport = controller(CAR_PACKAGE, pid = 2)

        grants.connected(browser)
        grants.connected(transport)
        grants.disconnected(browser)

        assertEquals(listOf(UriGrant.Granted(CAR_PACKAGE, covers, PREFIX_READ)), uriGrants.events)

        grants.disconnected(transport)

        assertEquals(
            UriGrant.Revoked(CAR_PACKAGE, covers, Intent.FLAG_GRANT_READ_URI_PERMISSION),
            uriGrants.events.last(),
        )
    }

    @Test
    fun controllersInThisAppAreGrantedNothing() {
        val grants = ArtworkReadGrants(uriGrants)
        val own = controller(uriGrants.packageName, pid = 1)
        val unnamed = controller(MediaSession.ControllerInfo.LEGACY_CONTROLLER_PACKAGE_NAME, pid = 2)

        grants.connected(own)
        grants.connected(unnamed)
        grants.disconnected(own)
        grants.disconnected(unnamed)

        assertEquals(emptyList<UriGrant>(), uriGrants.events)
    }

    /** Media3 reports no disconnect for a legacy browser or for controllers
     *  connected when the session is released. */
    @Test
    fun closingTheSessionRevokesEveryPackageStillConnected() {
        val grants = ArtworkReadGrants(uriGrants)
        grants.connected(controller(CAR_PACKAGE, pid = 1))
        grants.connected(controller(WATCH_PACKAGE, pid = 2))

        grants.revokeAll()

        assertEquals(
            setOf(
                UriGrant.Revoked(CAR_PACKAGE, covers, Intent.FLAG_GRANT_READ_URI_PERMISSION),
                UriGrant.Revoked(WATCH_PACKAGE, covers, Intent.FLAG_GRANT_READ_URI_PERMISSION),
            ),
            uriGrants.events.filterIsInstance<UriGrant.Revoked>().toSet(),
        )
    }

    private fun callback(grants: ArtworkReadGrants): BaeLibrarySessionCallback =
        BaeLibrarySessionCallback(
            LibraryBrowseTree(
                Library(FakeAppHandle()),
                { BrowseLabels("Library", "Albums", "Composers") },
                { Uri.parse("content://test/cover/${it.id}") },
                reportBug = { operation, error -> throw AssertionError("unexpected bug in $operation", error) },
            ),
            grants,
            CoroutineScope(Dispatchers.Unconfined),
        ) { "not found: $it" }

    private fun session(callback: BaeLibrarySessionCallback): MediaLibrarySession =
        MediaLibrarySession.Builder(RuntimeEnvironment.getApplication(), IdlePlayer(), callback).build()

    private fun controller(
        packageName: String,
        pid: Int,
    ): MediaSession.ControllerInfo =
        MediaSession.ControllerInfo.createTestOnlyControllerInfo(
            packageName,
            pid,
            10_000 + pid,
            1,
            1,
            false,
            Bundle.EMPTY,
        )
}

private sealed interface UriGrant {
    data class Granted(
        val packageName: String,
        val uri: Uri,
        val flags: Int,
    ) : UriGrant

    data class Revoked(
        val packageName: String,
        val uri: Uri,
        val flags: Int,
    ) : UriGrant
}

/** The app context, recording the URI grants made through it. */
private class RecordingUriGrants(
    base: Context,
) : ContextWrapper(base) {
    val events = mutableListOf<UriGrant>()

    override fun grantUriPermission(
        toPackage: String,
        uri: Uri,
        modeFlags: Int,
    ) {
        events += UriGrant.Granted(toPackage, uri, modeFlags)
    }

    override fun revokeUriPermission(
        toPackage: String,
        uri: Uri,
        modeFlags: Int,
    ) {
        events += UriGrant.Revoked(toPackage, uri, modeFlags)
    }
}

/** A player with nothing to play, for a session the test connects controllers to. */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
private class IdlePlayer : SimpleBasePlayer(Looper.getMainLooper()) {
    override fun getState(): State =
        State
            .Builder()
            .setAvailableCommands(Player.Commands.EMPTY)
            .build()
}
