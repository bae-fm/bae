package fm.bae.app.playback

import android.app.PendingIntent
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.mainActivityIntent

private const val TAG = "bae.PlaybackService"
private val logger = BaeLogger(TAG)
private const val SESSION_ACTIVITY_REQUEST_CODE = 1

/**
 * Media3 session service that hosts the [MediaSession] over the open library's
 * [BaeCorePlayer]. Hosting the session here (rather than in a composable or the
 * activity) keeps playback transport alive across configuration changes and
 * surfaces lock-screen / notification controls.
 *
 * The player itself is a pure projection of bae-core's playback state and is
 * owned by the open [fm.bae.app.OpenLibrary] (where the bridge handle, library,
 * and event subscription live). The service reads it from the session holder so
 * there's a single player instance: the in-app UI, the OS media controls, and
 * the core event stream all drive the same one.
 */
class PlaybackService : MediaSessionService() {
    private var mediaSession: MediaSession? = null

    override fun onCreate() {
        super.onCreate()
        val player = AppSessionHolder.currentSession()?.playback
        if (player == null) {
            // The service only runs while a library is open and playback has
            // begun — BaeCorePlayer.ensurePlaybackService starts us. If the
            // library was closed between that start and now, there's nothing to
            // host.
            logger.error("onCreate with no open session; stopping")
            stopSelf()
            return
        }
        val session =
            MediaSession
                .Builder(this, player)
                .setSessionActivity(sessionActivity())
                .build()
        addSession(session)
        mediaSession = session
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = mediaSession

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
