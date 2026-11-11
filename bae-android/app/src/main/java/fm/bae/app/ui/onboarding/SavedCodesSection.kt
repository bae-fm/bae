package fm.bae.app.ui.onboarding

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.BaeAlertDialog
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.DestructiveTextButton
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.NeutralTextButton
import fm.bae.app.ui.components.RowCard
import fm.bae.app.ui.components.SectionHeading
import fm.bae.app.ui.storageLocationLabel
import uniffi.bae_bridge.BridgeDropboxAccess
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeOAuthAccess
import uniffi.bae_bridge.BridgeSavedRestoreCode
import uniffi.bae_bridge.BridgeSavedRestoreCodeInfo
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeUnreadableSavedCode
import uniffi.bae_bridge.NoHandle

/** The widest the saved-code list grows, on a tablet or a phone held sideways. */
private val savedCodesWidth = 400.dp

/** What the saved-code list asks of the screen showing it. */
internal class SavedCodesCallbacks(
    val onOpen: (SavedRestoreCode) -> Unit,
    val onDelete: (SavedRestoreEntry) -> Unit,
)

/**
 * The welcome's list of restore codes saved on this phone, read from coven
 * each time the welcome shows. Tapping a library restores it; Delete asks
 * first, then removes the code from this phone only. Nothing shows while the
 * list loads or when nothing is saved.
 */
@Composable
internal fun SavedCodesSection(
    codes: SavedRestoreCodes,
    onOpen: (SavedRestoreCode) -> Unit,
) {
    LaunchedEffect(codes) { codes.load() }
    SavedCodesContent(
        list = codes.list,
        deletingLibraryId = codes.deletingLibraryId,
        deleteError = codes.deleteError,
        callbacks = SavedCodesCallbacks(onOpen = onOpen, onDelete = codes::askToDelete),
    )
    codes.pendingDelete?.let { pending ->
        DeleteSavedCodeDialog(
            entry = pending,
            onConfirm = { codes.delete(pending) },
            onDismiss = codes::dismissDelete,
        )
    }
}

/**
 * The saved codes under their heading, with a failed delete above them; or
 * why they couldn't be read. While a delete runs, its row shows progress and
 * no row can be tapped.
 */
@Composable
private fun SavedCodesContent(
    list: SavedCodesList,
    deletingLibraryId: String?,
    deleteError: DisplayError?,
    callbacks: SavedCodesCallbacks,
) {
    val column = Modifier.widthIn(max = savedCodesWidth).fillMaxWidth().padding(bottom = ThemeSpace.section)
    when (list) {
        SavedCodesList.Loading -> {}

        is SavedCodesList.Failed -> {
            Column(modifier = column, verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
                Text(text = stringResource(R.string.onboarding_saved_codes_failed), style = ThemeText.heading.style)
                ErrorNotice(list.error)
            }
        }

        is SavedCodesList.Loaded -> {
            if (list.entries.isEmpty()) return
            Column(modifier = column, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                SectionHeading(stringResource(R.string.onboarding_saved_codes_title))
                Text(
                    text = stringResource(R.string.onboarding_saved_codes_explanation),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                deleteError?.let { error ->
                    Text(
                        text = stringResource(R.string.onboarding_saved_code_delete_failed),
                        style = ThemeText.heading.style,
                    )
                    ErrorNotice(error)
                }
                RowCard {
                    list.entries.forEach { entry ->
                        SavedCodeRow(
                            entry = entry,
                            enabled = deletingLibraryId == null,
                            callbacks = callbacks,
                        )
                    }
                }
            }
        }
    }
}

/**
 * One saved entry. A readable code shows the library's saved name over where
 * it is stored, which tells apart two libraries with one name, and the row
 * restores it; one bae can't read says why, with its library id, and only
 * deletes. Both have Delete, disabled while any delete runs: a local
 * keystore call that ends at once.
 */
@Composable
private fun SavedCodeRow(
    entry: SavedRestoreEntry,
    enabled: Boolean,
    callbacks: SavedCodesCallbacks,
) {
    val title: String
    val subtitle: String
    var onClick: (() -> Unit)? = null
    when (entry) {
        is SavedRestoreEntry.Readable -> {
            title = entry.saved.info.name
            subtitle = storageLocationLabel(entry.saved.info.location)
            if (enabled) onClick = { callbacks.onOpen(entry.saved) }
        }

        is SavedRestoreEntry.Unreadable -> {
            title = stringResource(R.string.onboarding_saved_code_unusable)
            subtitle = "${stringResource(entry.reason.labelRes())} · ${entry.libraryId}"
        }
    }
    CardRow(
        title = title,
        subtitle = subtitle,
        onClick = onClick,
        onClickLabel = stringResource(R.string.onboarding_restore_action),
    ) {
        DestructiveTextButton(onClick = { callbacks.onDelete(entry) }, enabled = enabled) {
            Text(stringResource(R.string.onboarding_saved_code_delete))
        }
    }
}

/**
 * Asks before deleting [entry]'s code, saying it goes from this phone only
 * and that the library and its storage stay.
 */
@Composable
private fun DeleteSavedCodeDialog(
    entry: SavedRestoreEntry,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val body =
        when (entry) {
            is SavedRestoreEntry.Readable -> {
                stringResource(R.string.onboarding_saved_code_delete_body, entry.saved.info.name)
            }

            is SavedRestoreEntry.Unreadable -> {
                stringResource(R.string.onboarding_saved_code_delete_body_unusable)
            }
        }
    BaeAlertDialog(
        onDismissRequest = onDismiss,
        title = stringResource(R.string.onboarding_saved_code_delete_title),
        message = body,
        confirmButton = {
            DestructiveTextButton(onClick = onConfirm) {
                Text(stringResource(R.string.onboarding_saved_code_delete))
            }
        },
        dismissButton = {
            NeutralTextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** Why a saved code can't be used, as its row says it. */
@StringRes
private fun BridgeUnreadableSavedCode.labelRes(): Int =
    when (this) {
        BridgeUnreadableSavedCode.FORMAT -> R.string.onboarding_saved_code_reason_format
        BridgeUnreadableSavedCode.OTHER_LIBRARY -> R.string.onboarding_saved_code_reason_other_library
        BridgeUnreadableSavedCode.AMBIGUOUS -> R.string.onboarding_saved_code_reason_ambiguous
    }

/** Two libraries that share a name in different storage, then an entry bae can't read. */
private val previewEntries =
    listOf(
        SavedRestoreEntry.Readable(
            SavedRestoreCode(
                BridgeSavedRestoreCode(NoHandle),
                BridgeSavedRestoreCodeInfo(
                    name = "Home Library",
                    libraryId = "saved-01",
                    location =
                        BridgeStorageLocation.S3(
                            bucket = "music-library",
                            region = "us-east-1",
                            endpoint = "storage.example.com",
                            profile = null,
                        ),
                    oauthAccess = null,
                    hasS3Credentials = true,
                ),
            ),
        ),
        SavedRestoreEntry.Readable(
            SavedRestoreCode(
                BridgeSavedRestoreCode(NoHandle),
                BridgeSavedRestoreCodeInfo(
                    name = "Home Library",
                    libraryId = "saved-02",
                    location =
                        BridgeStorageLocation.Dropbox(
                            location = BridgeDropboxLocation.AppFolder,
                            accountLabel = "ana@example.com",
                            folderLabel = null,
                        ),
                    oauthAccess = BridgeOAuthAccess.Dropbox(BridgeDropboxAccess.APP_FOLDER),
                    hasS3Credentials = false,
                ),
            ),
        ),
        SavedRestoreEntry.Unreadable("6f1c2a9e-4b7d-4e2a-9c3f-2d8e5b1a7c40", BridgeUnreadableSavedCode.FORMAT),
    )

@Preview(showBackground = true)
@Composable
private fun SavedCodesPreview() {
    BaeTheme {
        Column(modifier = Modifier.padding(ThemeSpace.page)) {
            SavedCodesContent(
                list = SavedCodesList.Loaded(previewEntries),
                deletingLibraryId = null,
                deleteError = null,
                callbacks = SavedCodesCallbacks(onOpen = {}, onDelete = {}),
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun DeleteSavedCodePreview() {
    BaeTheme {
        DeleteSavedCodeDialog(entry = previewEntries.first(), onConfirm = {}, onDismiss = {})
    }
}
