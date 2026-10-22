package fm.bae.app

import android.content.Context
import android.icu.text.ListFormatter
import android.icu.text.NumberFormat
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeS3PrefixProblem
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.bridgeErrorLineKey
import uniffi.bae_bridge.bridgePlaybackErrorReasonKey
import uniffi.bae_bridge.bridgeSyncRetryOutcomeKey
import java.text.DateFormat
import java.util.Date

// Renders bae's typed error events for the current locale. The locale never
// crosses the bridge: core emits the typed reason plus a stable `core.*` catalog
// key; this resolves the localized line. The opaque diagnostic `detail` (the
// Rust error chain) is never translated: it rides beside the line in a
// [DisplayError], which offers it behind a details disclosure with a copy
// action. Mirrors macOS's `BridgeError+Localized` / iOS's `DisplayError`.
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
fun Context.localizedLine(error: BridgeException): String? =
    bridgeErrorLineKey(error)?.let { key ->
        // The line takes the counts and disc numbers its category carries,
        // formatted for the locale.
        when (val category = (error as? BridgeException.Diagnostic)?.category) {
            is BridgeErrorCategory.MetadataTrackCount -> {
                coreString(
                    key,
                    mapOf(
                        "folder_tracks" to category.folderTracks.toLong(),
                        "release_tracks" to category.releaseTracks.toLong(),
                    ),
                )
            }

            is BridgeErrorCategory.TiedMediums -> {
                val numbers = NumberFormat.getIntegerInstance(currentLocale())
                val discs = category.mediums.map { numbers.format(it.toLong()) }
                coreString(key, mapOf("discs" to ListFormatter.getInstance(currentLocale()).format(discs)))
            }

            // The line names the key prefix's folder at fault, and the
            // character it may not hold.
            is BridgeErrorCategory.S3Prefix -> {
                when (val problem = category.problem) {
                    is BridgeS3PrefixProblem.InvalidCharacter -> {
                        coreString(key, mapOf("segment" to problem.segment, "character" to problem.character))
                    }

                    is BridgeS3PrefixProblem.SegmentTooLong -> {
                        coreString(key, mapOf("segment" to problem.segment))
                    }

                    is BridgeS3PrefixProblem.EmptySegment -> {
                        coreString(key)
                    }
                }
            }

            else -> {
                coreString(key)
            }
        }
    }

/**
 * The localized, user-facing line for a [BridgePlaybackErrorReason]. The
 * actionable cases, where this device can't reach the audio, resolve their own
 * keyed line (core returns a key for exactly those); everything else renders
 * through the shared [BridgeException] path.
 */
fun Context.localizedLine(reason: BridgePlaybackErrorReason): String? =
    when (reason) {
        is BridgePlaybackErrorReason.SyncDisconnected,
        is BridgePlaybackErrorReason.UploadPending,
        is BridgePlaybackErrorReason.NotUploaded,
        is BridgePlaybackErrorReason.OriginalMissing,
        is BridgePlaybackErrorReason.AudioOutput,
        -> {
            // Core owns a key for exactly the actionable cases; non-null here.
            coreString(bridgePlaybackErrorReasonKey(reason)!!)
        }

        is BridgePlaybackErrorReason.Diagnostic -> {
            localizedLine(reason.error)
        }
    }

/** The diagnostic core wrote behind a failure: the Rust error chain of a diagnostic, nothing for a keyed one. */
val BridgeException.diagnosticDetail: String?
    get() = (this as? BridgeException.Diagnostic)?.detail

/** The diagnostic behind a playback failure, when it is one core could not name for the person. */
val BridgePlaybackErrorReason.diagnosticDetail: String?
    get() = (this as? BridgePlaybackErrorReason.Diagnostic)?.error?.diagnosticDetail

/** A [BridgeException] ready to show, or null when core says it has nothing to show — a cancellation. */
fun Context.displayError(error: BridgeException): DisplayError? =
    localizedLine(error)?.let { line -> DisplayError(line, error.diagnosticDetail) }

/** A [BridgePlaybackErrorReason] ready to show, or null when core says it has nothing to show. */
fun Context.displayError(reason: BridgePlaybackErrorReason): DisplayError? =
    localizedLine(reason)?.let { line -> DisplayError(line, reason.diagnosticDetail) }

/**
 * What a retry that didn't sync shows, or null when it synced. A failed pass
 * shows its error; the provider's cooldown and sync not running resolve the
 * line core keys for them, the cooldown's end formatted for the locale.
 */
fun Context.displayError(outcome: BridgeSyncRetryOutcome): DisplayError? =
    when (outcome) {
        is BridgeSyncRetryOutcome.Synced -> {
            null
        }

        is BridgeSyncRetryOutcome.Failed -> {
            displayError(outcome.error)
        }

        // Core owns a key for exactly these two; non-null here.
        is BridgeSyncRetryOutcome.CoolingDown -> {
            val until =
                DateFormat
                    .getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT, currentLocale())
                    .format(Date.from(outcome.until))
            DisplayError(coreString(bridgeSyncRetryOutcomeKey(outcome)!!, mapOf("until" to until)))
        }

        is BridgeSyncRetryOutcome.NotSyncing -> {
            DisplayError(coreString(bridgeSyncRetryOutcomeKey(outcome)!!))
        }
    }

/**
 * Resolves a typed error event to what a person is shown. Production binds a
 * [Context] ([LocaleErrorDisplays]); unit tests pass a fixed one so the reducer
 * stays plain-JVM-testable — the same reason playback goes through a
 * [fm.bae.app.playback.PlaybackEventSink] rather than a live player.
 */
interface ErrorDisplays {
    /** Null when core says the failure has nothing to show. */
    fun display(reason: BridgePlaybackErrorReason): DisplayError?

    /** Null when core says the failure has nothing to show. */
    fun display(error: BridgeException): DisplayError?

    /** Null when the retry synced, or its failure has nothing to show. */
    fun display(outcome: BridgeSyncRetryOutcome): DisplayError?
}

/** The production resolver: renders for the device locale via the Core catalog. */
class LocaleErrorDisplays(
    private val context: Context,
) : ErrorDisplays {
    override fun display(reason: BridgePlaybackErrorReason) = context.displayError(reason)

    override fun display(error: BridgeException) = context.displayError(error)

    override fun display(outcome: BridgeSyncRetryOutcome) = context.displayError(outcome)
}

/**
 * Run a user-triggered bridge action, preserving coroutine cancellation while
 * turning its expected failure — a typed bridge error — into what [showError]
 * shows. Any other failure is a bug and propagates to the coroutine scope's
 * bug handler.
 */
internal suspend fun performBridgeAction(
    logger: BaeLogger,
    operation: String,
    errors: ErrorDisplays,
    showError: (DisplayError?) -> Unit,
    action: suspend () -> Unit,
) {
    try {
        action()
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("$operation failed", e)
        showError(errors.display(e))
    }
}
