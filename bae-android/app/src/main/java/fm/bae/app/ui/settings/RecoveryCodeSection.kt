package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.displayError
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.RecoveryCodeSection")

/** Where the recovery code's reveal stands. */
internal sealed interface RecoveryCodeState {
    data object Hidden : RecoveryCodeState

    data object Loading : RecoveryCodeState

    /** The code exactly as coven formats it. */
    data class Shown(
        val code: String,
    ) : RecoveryCodeState

    data class Failed(
        val error: DisplayError,
    ) : RecoveryCodeState
}

/**
 * The recovery code, hidden until the person asks for it, then generated and
 * shown until they hide it. Hiding cancels a generation still running, so a
 * late result never shows a code the person already put away.
 */
internal class RecoveryCodeReveal(
    private val scope: CoroutineScope,
    private val generate: suspend () -> String,
    private val errorDisplay: (BridgeException) -> DisplayError?,
) {
    var state by mutableStateOf<RecoveryCodeState>(RecoveryCodeState.Hidden)
        private set
    private var generating: Job? = null

    /** Generate the code and show it; a generation already running is replaced. */
    fun show() {
        generating?.cancel()
        state = RecoveryCodeState.Loading
        generating =
            scope.launch {
                val next =
                    try {
                        RecoveryCodeState.Shown(generate())
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: BridgeException) {
                        // A run this reveal replaced or hid owns no state any more, whatever core reported for it.
                        ensureActive()
                        logger.error("Failed to generate the recovery code", e)
                        // A failure with nothing to show is a cancellation from core; the person can ask again.
                        errorDisplay(e)?.let(RecoveryCodeState::Failed) ?: RecoveryCodeState.Hidden
                    }
                ensureActive()
                state = next
            }
    }

    fun hide() {
        generating?.cancel()
        generating = null
        state = RecoveryCodeState.Hidden
    }
}

/**
 * The person's recovery code, for when they lose every device. Adding a
 * device while one is at hand goes through Add device in People instead. The
 * reveal lives as long as Settings does, so leaving Settings hides the code.
 */
@Composable
internal fun RecoveryCodeSection(
    needsS3Key: Boolean,
    generate: suspend () -> String,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val reveal = remember { RecoveryCodeReveal(scope, generate) { context.displayError(it) } }
    RecoveryCodeSectionContent(
        needsS3Key = needsS3Key,
        state = reveal.state,
        onShow = reveal::show,
        onHide = reveal::hide,
    )
}

@Composable
private fun RecoveryCodeSectionContent(
    needsS3Key: Boolean,
    state: RecoveryCodeState,
    onShow: () -> Unit,
    onHide: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.recovery_code_title))
        Text(
            text = stringResource(R.string.recovery_code_explanation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (needsS3Key) {
            Text(
                text = stringResource(R.string.recovery_code_s3_key),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        when (state) {
            RecoveryCodeState.Hidden -> {
                ShowCodeButton(onShow)
            }

            RecoveryCodeState.Loading -> {
                CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.large))
            }

            is RecoveryCodeState.Shown -> {
                ShownCode(code = state.code, onHide = onHide)
            }

            is RecoveryCodeState.Failed -> {
                ErrorDetailDisclosure(state.error)
                ShowCodeButton(onShow)
            }
        }
    }
}

@Composable
private fun ShowCodeButton(onShow: () -> Unit) {
    OutlinedButton(onClick = onShow) {
        Text(stringResource(R.string.recovery_code_show))
    }
}

@Composable
private fun ShownCode(
    code: String,
    onHide: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    SelectionContainer {
        Text(text = code, style = ThemeText.mono.style)
    }
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        TextButton(onClick = { clipboard.setText(AnnotatedString(code)) }) {
            Text(stringResource(R.string.copy))
        }
        TextButton(onClick = onHide) {
            Text(stringResource(R.string.hide))
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeSectionHiddenPreview() {
    BaeTheme {
        RecoveryCodeSectionContent(needsS3Key = true, state = RecoveryCodeState.Hidden, onShow = {}, onHide = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeSectionLoadingPreview() {
    BaeTheme {
        RecoveryCodeSectionContent(needsS3Key = false, state = RecoveryCodeState.Loading, onShow = {}, onHide = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeSectionShownPreview() {
    BaeTheme {
        RecoveryCodeSectionContent(
            needsS3Key = true,
            state = RecoveryCodeState.Shown("CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"),
            onShow = {},
            onHide = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RecoveryCodeSectionFailedPreview() {
    BaeTheme {
        RecoveryCodeSectionContent(
            needsS3Key = false,
            state =
                RecoveryCodeState.Failed(
                    DisplayError("Couldn’t reach your storage.", "S3: Network: dispatch failure"),
                ),
            onShow = {},
            onHide = {},
        )
    }
}
