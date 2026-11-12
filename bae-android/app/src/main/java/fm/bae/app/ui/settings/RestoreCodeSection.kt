package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.displayError
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.rememberCopyText
import fm.bae.app.ui.components.withWordGap
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.RestoreCodeSection")

/** Where the restore code's reveal stands. */
internal sealed interface RestoreCodeState {
    /** The screen isn't open: no code is held. */
    data object Closed : RestoreCodeState

    data object Loading : RestoreCodeState

    /** The code in groups of four, held for Copy and Print and shown only as its first groups. */
    data class Masked(
        val code: String,
    ) : RestoreCodeState

    /** The code in groups of four, as core formats it. */
    data class Shown(
        val code: String,
    ) : RestoreCodeState

    data class Failed(
        val error: DisplayError,
    ) : RestoreCodeState
}

/**
 * The restore code from the screen opening to its closing: made when the
 * screen opens and held masked, so Copy and Print work at once; Show reveals
 * it and Hide masks it again. Closing drops it and cancels a generation still
 * running, so a late result never shows a code for a screen already closed.
 */
internal class RestoreCodeReveal(
    private val scope: CoroutineScope,
    private val generate: suspend () -> String,
    private val errorDisplay: (BridgeException) -> DisplayError?,
) {
    var state by mutableStateOf<RestoreCodeState>(RestoreCodeState.Closed)
        private set
    private var generating: Job? = null

    /** Make the code and hold it masked; a generation already running is replaced. */
    fun open() {
        generating?.cancel()
        state = RestoreCodeState.Loading
        generating =
            scope.launch {
                val next =
                    try {
                        RestoreCodeState.Masked(generate())
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: BridgeException) {
                        // A run this reveal replaced or closed owns no state any more, whatever core reported for it.
                        ensureActive()
                        logger.error("Failed to generate the restore code", e)
                        // A failure with nothing to show is a cancellation from core; the screen can open again.
                        errorDisplay(e)?.let(RestoreCodeState::Failed) ?: RestoreCodeState.Closed
                    }
                ensureActive()
                state = next
            }
    }

    fun show() {
        (state as? RestoreCodeState.Masked)?.let { state = RestoreCodeState.Shown(it.code) }
    }

    fun hide() {
        (state as? RestoreCodeState.Shown)?.let { state = RestoreCodeState.Masked(it.code) }
    }

    fun close() {
        generating?.cancel()
        generating = null
        state = RestoreCodeState.Closed
    }
}

/**
 * The restore code's reveal for a screen: made once [enabled], held masked until the screen leaves, so a section that
 * scrolls out of a lazy list and back shows the same code and never makes it again. Leaving the screen drops the code.
 */
@Composable
internal fun rememberRestoreCodeReveal(
    enabled: Boolean,
    generate: suspend () -> String,
): RestoreCodeReveal {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val reveal = remember { RestoreCodeReveal(scope, generate) { context.displayError(it) } }
    DisposableEffect(reveal, enabled) {
        if (enabled) reveal.open()
        onDispose { reveal.close() }
    }
    return reveal
}

/**
 * The person's restore code, for when they lose every device: [reveal] holds it masked, with Show and Hide, Copy and
 * Print. Adding a device while one is at hand goes through Add device in People instead. [location] is where the
 * library is stored, for the printed page.
 */
@Composable
internal fun RestoreCodeSection(
    reveal: RestoreCodeReveal,
    needsS3Key: Boolean,
    location: String,
) {
    val context = LocalContext.current
    RestoreCodeBody(
        needsS3Key = needsS3Key,
        state = reveal.state,
        onShow = reveal::show,
        onHide = reveal::hide,
        onPrint = { code -> printRestoreCode(context, RestoreCodePage.of(context, code, location)) },
    )
}

/** The section over a reveal of its own, made when the section opens and dropped when it leaves the screen. */
@Composable
internal fun RestoreCodeSection(
    needsS3Key: Boolean,
    location: String,
    generate: suspend () -> String,
) {
    RestoreCodeSection(
        reveal = rememberRestoreCodeReveal(enabled = true, generate = generate),
        needsS3Key = needsS3Key,
        location = location,
    )
}

/** The restore code under its own heading, as a page that holds other things shows it. */
@Composable
internal fun RestoreCodeSectionContent(
    needsS3Key: Boolean,
    state: RestoreCodeState,
    onShow: () -> Unit,
    onHide: () -> Unit,
    onPrint: (String) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        AndroidSection(text = stringResource(R.string.restore_code_title))
        RestoreCodeBody(needsS3Key = needsS3Key, state = state, onShow = onShow, onHide = onHide, onPrint = onPrint)
    }
}

/**
 * What the restore code shows: the code in its card, Show or Hide, Copy and Print under it, then the notes that say
 * what it is for; a spinner while it is made, or why it couldn't be.
 */
@Composable
internal fun RestoreCodeBody(
    needsS3Key: Boolean,
    state: RestoreCodeState,
    onShow: () -> Unit,
    onHide: () -> Unit,
    onPrint: (String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        when (state) {
            RestoreCodeState.Closed -> {
                Unit
            }

            RestoreCodeState.Loading -> {
                AndroidSpinner(text = stringResource(R.string.restore_code_making))
            }

            is RestoreCodeState.Masked -> {
                CodeBox(code = state.code, shown = false, onToggle = onShow, onPrint = onPrint)
                Notes(needsS3Key)
            }

            is RestoreCodeState.Shown -> {
                CodeBox(code = state.code, shown = true, onToggle = onHide, onPrint = onPrint)
                Notes(needsS3Key)
            }

            is RestoreCodeState.Failed -> {
                AndroidNotice(state.error)
            }
        }
    }
}

/** The notes under the buttons, each [ThemeSpace.related] below what is above it. */
@Composable
private fun Notes(needsS3Key: Boolean) {
    AndroidNote(
        text = stringResource(R.string.restore_code_explanation),
        modifier = Modifier.padding(top = ThemeSpace.related),
    )
    if (needsS3Key) {
        AndroidNote(
            text = stringResource(R.string.restore_code_s3_key),
            modifier = Modifier.padding(top = ThemeSpace.related),
        )
    }
}

/** The code, masked or shown, in its card, with Show or Hide, Copy and Print under it. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CodeBox(
    code: String,
    shown: Boolean,
    onToggle: () -> Unit,
    onPrint: (String) -> Unit,
) {
    val copy = rememberCopyText()
    val hiddenLabel = stringResource(R.string.restore_code_partly_hidden)
    AndroidCard {
        if (shown) {
            SelectionContainer {
                Text(
                    text = code.withWordGap(),
                    style = ThemeText.mono.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(ThemeSpace.edge),
                )
            }
        } else {
            Text(
                text = maskedRestoreCode(code).withWordGap(),
                style = ThemeText.mono.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier =
                    Modifier
                        .padding(ThemeSpace.edge)
                        .clearAndSetSemantics { contentDescription = hiddenLabel },
            )
        }
    }
    FlowRow(
        modifier = Modifier.padding(top = ThemeSpace.group),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        AndroidButton(kind = ButtonKind.OUTLINED, onClick = onToggle) {
            Text(stringResource(if (shown) R.string.hide else R.string.restore_code_show))
        }
        AndroidButton(kind = ButtonKind.OUTLINED, onClick = { copy(code) }) {
            Text(stringResource(R.string.copy))
        }
        AndroidButton(kind = ButtonKind.OUTLINED, onClick = { onPrint(code) }) {
            Text(stringResource(R.string.restore_code_print))
        }
    }
}

/**
 * [grouped], a code in groups of four, with every group after the second as
 * dots: the prefix and the first four characters stay, which tells one code
 * from another without it being readable over a shoulder.
 */
internal fun maskedRestoreCode(grouped: String): String =
    grouped
        .split(' ')
        .filter { it.isNotEmpty() }
        .mapIndexed { index, group -> if (index < VISIBLE_GROUPS) group else MASK }
        .joinToString(" ")

private const val VISIBLE_GROUPS = 2
private const val MASK = "••••"

/** A restore code in groups of four, for the previews; never a library's. */
private const val PREVIEW_CODE =
    "CVR1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567 ABCD EFGH IJKL MNOP QRST"

/** The section on a Dropbox library with a fixture code, masked or shown, for the screenshot gallery. */
@Composable
internal fun RestoreCodeScene(shown: Boolean) {
    RestoreCodeSectionContent(
        needsS3Key = false,
        state = if (shown) RestoreCodeState.Shown(PREVIEW_CODE) else RestoreCodeState.Masked(PREVIEW_CODE),
        onShow = {},
        onHide = {},
        onPrint = {},
    )
}

@Preview(showBackground = true)
@Composable
private fun RestoreCodeSectionMaskedPreview() {
    PreviewTheme {
        RestoreCodeSectionContent(
            needsS3Key = true,
            state = RestoreCodeState.Masked(PREVIEW_CODE),
            onShow = {},
            onHide = {},
            onPrint = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RestoreCodeSectionLoadingPreview() {
    PreviewTheme {
        RestoreCodeSectionContent(
            needsS3Key = false,
            state = RestoreCodeState.Loading,
            onShow = {},
            onHide = {},
            onPrint = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RestoreCodeSectionShownPreview() {
    PreviewTheme {
        RestoreCodeSectionContent(
            needsS3Key = true,
            state = RestoreCodeState.Shown(PREVIEW_CODE),
            onShow = {},
            onHide = {},
            onPrint = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun RestoreCodeSectionFailedPreview() {
    PreviewTheme {
        RestoreCodeSectionContent(
            needsS3Key = false,
            state =
                RestoreCodeState.Failed(
                    DisplayError("Could not reach the cloud.", "S3: Network: dispatch failure"),
                ),
            onShow = {},
            onHide = {},
            onPrint = {},
        )
    }
}
