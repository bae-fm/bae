package fm.bae.app.playback

import android.app.PendingIntent
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.mainActivityIntent
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel

private const val TAG = "bae.PlaybackService"
private val logger = BaeLogger(TAG)
private const val SESSION_ACTIVITY_REQUEST_CODE = 1

/**
 * Media3 service that hosts the [MediaLibrarySession] over the open library's
 * [BaeCorePlayer]. Hosting the session here (rather than in a composable or the
 * activity) keeps playback transport alive across configuration changes and
 * surfaces lock-screen / notification / Android Auto / Bluetooth controls.
 *
 * It is a [MediaLibraryService] (not a bare `MediaSessionService`) so head units
 * and other browse clients get a navigable library — root → Albums / Composers,
 * drilling to tracks — served by [LibraryBrowseTree] through the same paged
 * bridge reads the in-app browser uses. `MediaLibraryService` extends
 * `MediaSessionService`, so all the transport, focus, foreground, and queue
 * behavior is unchanged; only the session type and its browse callback are new.
 *
 * The player itself is a pure projection of bae-core's playback state and is
 * owned by the open [fm.bae.app.OpenLibrary] (where the bridge handle, library,
 * and event subscription live). The service reads it from the session holder so
 * there's a single player instance: the in-app UI, the OS media controls, the
 * browse tree, and the core event stream all drive the same one.
 */
class PlaybackService : MediaLibraryService() {
    private var mediaSession: MediaLibrarySession? = null
    private var browseTree: LibraryBrowseTree? = null

    // Browse reads (onGetChildren / onGetItem) run here — off the main thread,
    // since they hit the bridge/DB — for the service's lifetime.
    private val browseScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onCreate() {
        super.onCreate()
        val open = AppSessionHolder.currentSession()
        val player = open?.playback
        if (open == null || player == null) {
            // The service only runs while a library is open and playback has
            // begun — BaeCorePlayer.ensurePlaybackService starts us. If the
            // library was closed between that start and now, there's nothing to
            // host.
            logger.error("onCreate with no open session; stopping")
            stopSelf()
            return
        }
        var callback: BaeLibrarySessionCallback? = null
        val tree =
            LibraryBrowseTree(
                library = open.library,
                labels = {
                    BrowseLabels(
                        albums = getString(fm.bae.app.R.string.library_mode_albums),
                        composers = getString(fm.bae.app.R.string.library_mode_composers),
                    )
                },
                artworkUri = { image -> ArtworkContentProvider.uriFor(this, image) },
                scope = browseScope,
                onChildrenChanged = { parentId, count ->
                    mediaSession?.notifyChildrenChanged(parentId, count, null)
                },
                onSearchResultChanged = { query, count ->
                    callback?.searchResultsChanged(query, count)
                },
            )
        callback = BaeLibrarySessionCallback(tree, browseScope)
        val session =
            MediaLibrarySession
                .Builder(this, player, callback)
                .setSessionActivity(sessionActivity())
                .build()
        addSession(session)
        browseTree = tree
        mediaSession = session
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? = mediaSession

    override fun onDestroy() {
        // Release the MediaSession only — never the player. The player is owned
        // by the open OpenLibrary and outlives this service: the service starts
        // when playback begins and may be torn down and recreated (a fresh
        // session over the same player) within one library session, and the
        // system can kill the service on its own. Releasing the player here would
        // break the in-app UI, which holds the same instance, and a later restart
        // would build a session over a released player.
        mediaSession?.run {
            removeSession(this)
            release()
        }
        mediaSession = null
        browseTree?.close()
        browseTree = null
        browseScope.cancel()
        super.onDestroy()
    }

    private fun sessionActivity(): PendingIntent =
        PendingIntent.getActivity(
            this,
            SESSION_ACTIVITY_REQUEST_CODE,
            mainActivityIntent(this),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
}
