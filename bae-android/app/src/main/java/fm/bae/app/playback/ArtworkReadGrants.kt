package fm.bae.app.playback

import android.content.Context
import android.content.Intent
import androidx.media3.session.MediaSession
import fm.bae.app.BaeLogger

private const val TAG = "bae.ArtworkReadGrants"
private val logger = BaeLogger(TAG)

/** Read access to the granted URI and every URI under it. */
private const val COVER_GRANT_FLAGS =
    Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION

/**
 * Lets the media controllers connected to the session read cover URIs from the
 * unexported [ArtworkContentProvider]. A browse item's cover reaches Android
 * Auto, an Automotive media center or a Media3 browser as a bare `content://`
 * URI the controller opens itself, and neither Media3 nor the framework grants
 * the controller access to it. So a controller's package holds a read grant on
 * [ArtworkContentProvider.coversUri] while any of its controllers is
 * connected; the grant is revoked when its last controller disconnects or the
 * session closes ([revokeAll]).
 *
 * Controllers in this app read the provider as its owner and get no grant. A
 * controller named [MediaSession.ControllerInfo.LEGACY_CONTROLLER_PACKAGE_NAME]
 * names no package to grant to; the session's now-playing cover reaches such
 * controllers as a bitmap Media3 loads in this process.
 *
 * Media3 calls the session callback on the session's application thread, so
 * every call here runs on that one thread.
 */
internal class ArtworkReadGrants(
    private val context: Context,
) {
    private val covers = ArtworkContentProvider.coversUri(context)

    /** The connected controllers of each package that holds a grant. */
    private val connected = mutableMapOf<String, MutableSet<MediaSession.ControllerInfo>>()

    /** [controller] was accepted: its package can read covers while it stays connected. */
    fun connected(controller: MediaSession.ControllerInfo) {
        val packageName = grantablePackage(controller) ?: return
        val controllers = connected.getOrPut(packageName) { mutableSetOf() }
        if (controllers.isEmpty()) {
            context.grantUriPermission(packageName, covers, COVER_GRANT_FLAGS)
            logger.debug("granted $packageName read access to $covers")
        }
        controllers += controller
    }

    /** [controller] disconnected: its package loses access once none of its controllers is connected. */
    fun disconnected(controller: MediaSession.ControllerInfo) {
        val packageName = grantablePackage(controller) ?: return
        val controllers = connected[packageName]
        if (controllers == null || !controllers.remove(controller)) {
            logger.warning("$controller disconnected without having connected")
            return
        }
        if (controllers.isEmpty()) {
            connected.remove(packageName)
            revoke(packageName)
        }
    }

    /** The session closed: no controller of it can read covers. Media3 reports
     *  no disconnect for the controllers still connected when a session is
     *  released, nor ever for a legacy browser (Android Auto's). */
    fun revokeAll() {
        connected.keys.forEach(::revoke)
        connected.clear()
    }

    /** Revoke [packageName]'s grant on the cover prefix (a revoke takes only the
     *  access mode; it removes grants on the URI and every path under it). */
    private fun revoke(packageName: String) {
        context.revokeUriPermission(packageName, covers, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        logger.debug("revoked $packageName read access to $covers")
    }

    /** The package to grant [controller]'s reads to, or null when it needs or takes none. */
    private fun grantablePackage(controller: MediaSession.ControllerInfo): String? =
        when (controller.packageName) {
            context.packageName -> {
                null
            }

            MediaSession.ControllerInfo.LEGACY_CONTROLLER_PACKAGE_NAME -> {
                logger.debug("no cover grant for $controller: it names no package")
                null
            }

            else -> {
                controller.packageName
            }
        }
}
