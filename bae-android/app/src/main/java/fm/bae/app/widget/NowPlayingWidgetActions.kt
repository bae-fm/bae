package fm.bae.app.widget

import android.content.Context
import android.content.Intent
import androidx.glance.GlanceId
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.appwidget.action.ActionCallback
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.mainActivityIntent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val TAG = "bae.NowPlayingWidgetActions"
private val logger = BaeLogger(TAG)

internal const val COMMAND_TOGGLE = "toggle"
internal const val COMMAND_NEXT = "next"

internal val widgetCommandKey = ActionParameters.Key<String>("bae.widget.command")

internal fun widgetCommand(command: String): ActionParameters = actionParametersOf(widgetCommandKey to command)

/**
 * Runs a widget transport tap against the live playback session. The callback
 * fires in the app process (Android brings it up if dead): while playback is
 * live the command drives the same [fm.bae.app.playback.BaeCorePlayer] the media
 * session and in-app UI drive, so it works with the app UI gone but the service
 * alive. With no open session — the service is dead, or this is a cold start —
 * there is nothing to drive, so the tap opens the app to resume there instead.
 */
class NowPlayingWidgetTransportAction : ActionCallback {
    override suspend fun onAction(
        context: Context,
        glanceId: GlanceId,
        parameters: ActionParameters,
    ) {
        val command = parameters[widgetCommandKey]
        if (command == null) {
            logger.warning("widget transport tap with no command")
            return
        }
        val session = AppSessionHolder.currentSession()
        if (session == null) {
            logger.debug("widget transport tap ($command) with no live session; opening app")
            context.startActivity(
                mainActivityIntent(context).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            )
            return
        }
        withContext(Dispatchers.Main.immediate) {
            when (command) {
                COMMAND_TOGGLE -> session.playback.togglePlayPause()
                COMMAND_NEXT -> session.appHandle.nextTrack()
                else -> logger.warning("unknown widget transport command $command")
            }
        }
    }
}
