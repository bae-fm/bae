package fm.bae.app.playback

import androidx.media3.common.Player
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.QueueUpcomingCallback

fun interface QueuePageSubscription {
    fun cancel()
}

class QueuePageSource(
    val subscribe: (offset: UInt, limit: UInt, callback: QueueUpcomingCallback) -> QueuePageSubscription,
) {
    constructor(handle: AppHandle) : this(
        subscribe = { offset, limit, callback ->
            val subscription = handle.subscribeQueueUpcomingPage(offset, limit, callback)
            QueuePageSubscription(subscription::cancel)
        },
    )
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
internal fun availableCommands(
    hasNext: Boolean,
    hasPrevious: Boolean,
): Player.Commands {
    val builder =
        Player.Commands
            .Builder()
            .add(Player.COMMAND_PLAY_PAUSE)
            .add(Player.COMMAND_STOP)
            .add(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM)
            .add(Player.COMMAND_SEEK_TO_MEDIA_ITEM)
            // Lets a browse client (Android Auto / a head unit) play a tapped
            // library item: its play request resolves to a single-item
            // setMediaItem, which routes to handleSetMediaItems. The core-driven
            // queue isn't editable through the raw media-item API, so
            // COMMAND_CHANGE_MEDIA_ITEMS (add/remove/move) stays unavailable.
            .add(Player.COMMAND_SET_MEDIA_ITEM)
            .add(Player.COMMAND_SET_REPEAT_MODE)
            .add(Player.COMMAND_GET_CURRENT_MEDIA_ITEM)
            .add(Player.COMMAND_GET_TIMELINE)
            .add(Player.COMMAND_GET_METADATA)

    listOf(
        hasNext to listOf(Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM),
        hasPrevious to listOf(Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM),
    ).forEach { (enabled, commands) ->
        if (enabled) {
            commands.forEach { builder.add(it) }
        }
    }

    return builder.build()
}
