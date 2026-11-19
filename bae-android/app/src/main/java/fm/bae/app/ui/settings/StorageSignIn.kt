package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidProgress
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.components.focusOnShow
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAccessKeyOutcome
import uniffi.bae_bridge.BridgeCancellation
import uniffi.bae_bridge.BridgeException

/**
 * Replacing the S3 key of the library on the Sync page: whether the S3 key dialog is open and what it shows, the call
 * that checks the key with S3 storage and uses it, and the line after a save that names the old key to delete. The
 * page owns it: Replace key and Sign in to S3 both open the same dialog. Nothing stores the old key's id; it goes when
 * the person dismisses the line or the page closes.
 */
internal class S3KeyReplacement(
    private val scope: CoroutineScope,
    private val useAccessKey: suspend (String, String, BridgeCancellation) -> BridgeAccessKeyOutcome,
    private val display: (BridgeException) -> DisplayError?,
    private val makeCancellation: () -> BridgeCancellation = { BridgeCancellation() },
) {
    /** What the open dialog shows. */
    sealed interface Sheet {
        /** The two fields, with why the last key failed when it did. */
        data class Fields(
            val failure: Failure?,
        ) : Sheet

        /** bae checks the key with S3 storage; only Cancel shows. */
        data object Checking : Sheet
    }

    /** Why the last key changed nothing. */
    sealed interface Failure {
        /** S3 storage refused the key. */
        data object Refused : Failure

        /** The check could not finish, or another failure, in its own line. */
        data class Failed(
            val error: DisplayError,
        ) : Failure
    }

    /** The line after a save: the key the library recorded before, or null when it already recorded the new one. */
    data class Saved(
        val oldKey: String?,
    )

    /** The open dialog's face; null while the dialog is closed. */
    var sheet by mutableStateOf<Sheet?>(null)
        private set

    /** The line after the last save, until the person dismisses it. */
    var saved by mutableStateOf<Saved?>(null)
        private set

    /**
     * The running call and its cancel. A call the person cancelled runs until its check stops, and its save still
     * shows the line.
     */
    private var running: Pair<Any, BridgeCancellation>? = null

    fun open() {
        sheet = Sheet.Fields(null)
    }

    /** Check the key with S3 storage and use it in place of the library's. */
    fun save(
        accessKeyId: String,
        secretAccessKey: String,
    ) {
        val call = Any()
        val cancellation = makeCancellation()
        running = call to cancellation
        sheet = Sheet.Checking
        scope.launch {
            val ended =
                try {
                    Ended.of(useAccessKey(accessKeyId, secretAccessKey, cancellation), display)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: BridgeException) {
                    Ended.Failed(display(e))
                }
            finish(call, ended)
        }
    }

    /**
     * Close the dialog. A check that runs stops, and nothing changes, unless it had already passed: then the save
     * finishes and shows its line.
     */
    fun cancel() {
        if (sheet == Sheet.Checking) running?.second?.cancel()
        sheet = null
    }

    fun dismissSaved() {
        saved = null
    }

    /** How a call ended, as the dialog tells it. */
    private sealed interface Ended {
        data class Saved(
            val oldKey: String?,
        ) : Ended

        data object Refused : Ended

        /** A failure with its line, or none for a failure core says has none to show. */
        data class Failed(
            val error: DisplayError?,
        ) : Ended

        data object Cancelled : Ended

        companion object {
            fun of(
                outcome: BridgeAccessKeyOutcome,
                display: (BridgeException) -> DisplayError?,
            ): Ended =
                when (outcome) {
                    is BridgeAccessKeyOutcome.Saved -> Saved(outcome.replaced)
                    BridgeAccessKeyOutcome.Refused -> Refused
                    is BridgeAccessKeyOutcome.Failed -> Failed(display(outcome.error))
                    BridgeAccessKeyOutcome.Cancelled -> Cancelled
                }
        }
    }

    private fun finish(
        call: Any,
        ended: Ended,
    ) {
        val current = running?.first === call
        if (current) running = null
        // Only the call the open dialog waits on changes the dialog.
        val shown = current && sheet == Sheet.Checking
        when (ended) {
            is Ended.Saved -> {
                saved = Saved(ended.oldKey)
                if (shown) sheet = null
            }

            Ended.Refused -> {
                if (shown) sheet = Sheet.Fields(Failure.Refused)
            }

            is Ended.Failed -> {
                if (shown) sheet = Sheet.Fields(ended.error?.let { Failure.Failed(it) })
            }

            Ended.Cancelled -> {
                Unit
            }
        }
    }
}

/**
 * The S3 key dialog that [sheet] describes: a new key for the library's S3 storage, which bae checks with the storage
 * before it uses it. The typed key stays while bae checks it, so a refused key shows the fields again as typed. Back
 * and a tap outside the dialog do what Cancel does.
 */
@Composable
internal fun S3KeyDialog(
    sheet: S3KeyReplacement.Sheet,
    onSave: (accessKeyId: String, secretAccessKey: String) -> Unit,
    onCancel: () -> Unit,
) {
    var accessKeyId by rememberSaveable { mutableStateOf("") }
    var secretAccessKey by rememberSaveable { mutableStateOf("") }
    AndroidDialog(
        onDismissRequest = onCancel,
        parts =
            s3KeyDialogParts(
                sheet = sheet,
                accessKeyId = accessKeyId,
                secretAccessKey = secretAccessKey,
                onAccessKeyIdChange = { accessKeyId = it.trim() },
                onSecretAccessKeyChange = { secretAccessKey = it.trim() },
                onSave = { onSave(accessKeyId, secretAccessKey) },
                onCancel = onCancel,
            ),
    )
}

/**
 * The parts of the S3 key dialog that [sheet] describes, its fields holding [accessKeyId] and [secretAccessKey]: while
 * bae checks the key, its words over a moving bar and only Cancel; else the fields under why the last key failed, and
 * Save, on once both hold text.
 */
@Composable
internal fun s3KeyDialogParts(
    sheet: S3KeyReplacement.Sheet,
    accessKeyId: String,
    secretAccessKey: String,
    onAccessKeyIdChange: (String) -> Unit,
    onSecretAccessKeyChange: (String) -> Unit,
    onSave: () -> Unit,
    onCancel: () -> Unit,
): DialogParts {
    val checking = sheet == S3KeyReplacement.Sheet.Checking
    return DialogParts(
        title = stringResource(R.string.s3_key_title),
        message = stringResource(R.string.s3_key_message),
        content = {
            if (checking) {
                AndroidProgress(title = stringResource(R.string.s3_key_checking))
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
                    (sheet as? S3KeyReplacement.Sheet.Fields)?.failure?.let { S3KeyFailureNotice(it) }
                    // The field the person types in next takes the focus as the fields show: the ID while it is
                    // empty, else the secret. Typing doesn't move it.
                    val focusesId = remember { accessKeyId.isEmpty() }
                    S3KeyFields(
                        accessKeyId = accessKeyId,
                        secretAccessKey = secretAccessKey,
                        onAccessKeyIdChange = onAccessKeyIdChange,
                        onSecretAccessKeyChange = onSecretAccessKeyChange,
                        accessKeyIdModifier = if (focusesId) Modifier.focusOnShow() else Modifier,
                        secretAccessKeyModifier = if (focusesId) Modifier else Modifier.focusOnShow(),
                    )
                }
            }
        },
        confirmButton = {
            if (!checking) {
                AndroidButton(
                    kind = ButtonKind.TEXT,
                    enabled = accessKeyId.isNotEmpty() && secretAccessKey.isNotEmpty(),
                    onClick = onSave,
                ) { Text(stringResource(R.string.save)) }
            }
        },
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = ButtonTone.NEUTRAL,
                onClick = onCancel,
            ) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** Why the last key changed nothing, above the fields. */
@Composable
private fun S3KeyFailureNotice(failure: S3KeyReplacement.Failure) {
    val title = stringResource(R.string.s3_key_refused_title)
    when (failure) {
        S3KeyReplacement.Failure.Refused -> {
            AndroidNotice(
                title = title,
                advice = stringResource(R.string.s3_key_refused),
                error = null,
                look = NoticeLook.QUIET,
            )
        }

        is S3KeyReplacement.Failure.Failed -> {
            AndroidNotice(title = title, advice = failure.error.line, error = failure.error, look = NoticeLook.QUIET)
        }
    }
}
