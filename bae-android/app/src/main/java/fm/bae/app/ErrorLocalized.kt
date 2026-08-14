package fm.bae.app

import android.content.Context
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.bridgeErrorLineKey
import uniffi.bae_bridge.bridgePlaybackErrorReasonKey

// Renders bae's typed error events for the current locale. The locale never
// crosses the bridge: core emits the typed reason plus a stable `core.*` catalog
// key; this resolves the localized line. The opaque diagnostic `detail` (the
// Rust error chain) is log-only and never translated, so it isn't surfaced as
// primary copy. Mirrors macOS's `BridgeError+Localized` / iOS's `DisplayError`.
//
// `BridgeException` is uniffi's Kotlin name for the bridge `BridgeError`.

/**
 * The localized, user-facing line for a typed [BridgeException], or null when it
 * has none to show — core's answer, not this app's. A cancellation is the user's
 * own doing and says nothing back to them.
 *
 * Null, never "": an empty string is not "nothing", it is a blank line, and it
 * only stayed invisible here because [fm.bae.app.data.UiEventAdapter] happened to
 * catch Cancelled separately before ever asking.
 */
fun Context.localizedLine(error: BridgeException): String? = bridgeErrorLineKey(error)?.let { coreString(it) }

/**
 * The localized, user-facing line for a [BridgePlaybackErrorReason]. The two
 * actionable cloud cases resolve their own keyed line (core returns a key for
 * exactly those); everything else renders through the shared [BridgeException]
 * path.
 */
fun Context.localizedLine(reason: BridgePlaybackErrorReason): String? =
    when (reason) {
        is BridgePlaybackErrorReason.SyncDisconnected,
        is BridgePlaybackErrorReason.UploadPending,
        -> {
            // Core owns a key for exactly the actionable cases; non-null here.
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
    /** Null when core says the failure has no line to show. */
    fun line(reason: BridgePlaybackErrorReason): String?

    /** Null when core says the failure has no line to show. */
    fun line(error: BridgeException): String?
}

/** The production resolver: renders for the device locale via the Core catalog. */
class LocaleErrorLines(
    private val context: Context,
) : ErrorLines {
    override fun line(reason: BridgePlaybackErrorReason) = context.localizedLine(reason)

    override fun line(error: BridgeException) = context.localizedLine(error)
}

/**
 * Run a user-triggered bridge action, preserving coroutine cancellation while
 * turning every operation failure into the library's visible error state.
 */
internal suspend fun performBridgeAction(
    logger: BaeLogger,
    operation: String,
    errors: ErrorLines,
    showError: (String?) -> Unit,
    action: suspend () -> Unit,
) {
    try {
        action()
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("$operation failed", e)
        showError(errors.line(e))
    } catch (e: Exception) {
        logger.error("$operation failed", e)
        showError(e.toString())
    }
}
