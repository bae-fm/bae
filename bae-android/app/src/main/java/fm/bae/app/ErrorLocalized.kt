package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.bridgeEntityNotFoundKey
import uniffi.bae_bridge.bridgeErrorCategoryKey
import uniffi.bae_bridge.bridgePlaybackErrorReasonKey

// Renders bae's typed error events for the current locale. The locale never
// crosses the bridge: core emits the typed reason plus a stable `core.*` catalog
// key; this resolves the localized line. The opaque diagnostic `detail` (the
// Rust error chain) is log-only and never translated, so it isn't surfaced as
// primary copy. Mirrors macOS's `BridgeError+Localized` / iOS's `DisplayError`.
//
// `BridgeException` is uniffi's Kotlin name for the bridge `BridgeError`.

/**
 * The localized, user-facing line for a typed [BridgeException]. `Cancelled`
 * shows nothing (the user dismissed it); `NotFound` and `Diagnostic` render the
 * generic line core owns the key for.
 */
fun Context.localizedLine(error: BridgeException): String =
    when (error) {
        is BridgeException.Cancelled -> ""
        is BridgeException.NotFound -> coreString(bridgeEntityNotFoundKey(error.entity))
        is BridgeException.Diagnostic -> coreString(bridgeErrorCategoryKey(error.category))
    }

/**
 * The localized, user-facing line for a [BridgePlaybackErrorReason]. The
 * actionable cloud case resolves its own keyed line (core returns a key for
 * exactly it); everything else renders through the shared [BridgeException]
 * path.
 */
fun Context.localizedLine(reason: BridgePlaybackErrorReason): String =
    when (reason) {
        is BridgePlaybackErrorReason.SyncDisconnected -> {
            // Core owns a key for exactly the actionable case; non-null here.
            coreString(bridgePlaybackErrorReasonKey(reason)!!)
        }

        is BridgePlaybackErrorReason.Diagnostic -> {
            localizedLine(reason.error)
        }
    }

/**
 * Resolves a typed error event to its localized one-line message. Production
 * binds a [Context] ([LocaleErrorLines]); unit tests pass a no-op so the reducer
 * stays plain-JVM-testable — the same reason playback goes through a
 * [fm.bae.app.playback.PlaybackEventSink] rather than a live player.
 */
interface ErrorLines {
    fun line(reason: BridgePlaybackErrorReason): String

    fun line(error: BridgeException): String
}

/** The production resolver: renders for the device locale via the Core catalog. */
class LocaleErrorLines(
    private val context: Context,
) : ErrorLines {
    override fun line(reason: BridgePlaybackErrorReason) = context.localizedLine(reason)

    override fun line(error: BridgeException) = context.localizedLine(error)
}
