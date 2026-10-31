package fm.bae.app.ui.onboarding

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.Eyebrow
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
                ErrorDetailDisclosure(list.error)
            }
        }

        is SavedCodesList.Loaded -> {
            if (list.entries.isEmpty()) return
            Column(modifier = column, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Eyebrow(stringResource(R.string.onboarding_saved_codes_title))
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
                    ErrorDetailDisclosure(error)
                }
                list.entries.forEach { entry ->
                    SavedCodeRow(
                        entry = entry,
                        isDeleting = deletingLibraryId == entry.libraryId,
                        enabled = deletingLibraryId == null,
                        callbacks = callbacks,
                    )
                }
            }
        }
    }
}

/**
 * One saved entry. A readable code shows the library's saved name over where
 * it is stored, which tells apart two libraries with one name, and the row
 * restores it; one bae can't read says why, with its library id, and only
 * deletes. Both have Delete, which shows progress while the delete runs.
 */
@Composable
private fun SavedCodeRow(
    entry: SavedRestoreEntry,
    isDeleting: Boolean,
    enabled: Boolean,
    callbacks: SavedCodesCallbacks,
) {
    val open =
        when (entry) {
            is SavedRestoreEntry.Readable -> {
                Modifier.clickable(
                    enabled = enabled,
                    onClickLabel = stringResource(R.string.onboarding_restore_action),
                ) { callbacks.onOpen(entry.saved) }
            }

            is SavedRestoreEntry.Unreadable -> {
                Modifier
            }
        }
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(ThemeRadius.card))
                .background(MaterialTheme.colorScheme.surfaceContainer)
                .then(open)
                .padding(start = ThemeSpace.edge, top = ThemeSpace.related, bottom = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.line)) {
            when (entry) {
                is SavedRestoreEntry.Readable -> {
                    RowLine(entry.saved.info.name, ThemeText.rowTitle, secondary = false)
                    RowLine(storageLocationLabel(entry.saved.info.location), ThemeText.detail, secondary = true)
                }

                is SavedRestoreEntry.Unreadable -> {
                    RowLine(
                        stringResource(R.string.onboarding_saved_code_unusable),
                        ThemeText.rowTitle,
                        secondary = false,
                    )
                    RowLine(stringResource(entry.reason.labelRes()), ThemeText.detail, secondary = true)
                    RowLine(entry.libraryId, ThemeText.detail, secondary = true)
                }
            }
        }
        Box(contentAlignment = Alignment.Center) {
            TextButton(
                onClick = { callbacks.onDelete(entry) },
                enabled = enabled,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                modifier = Modifier.alpha(if (isDeleting) 0f else 1f),
            ) {
                Text(stringResource(R.string.onboarding_saved_code_delete))
            }
            if (isDeleting) {
                CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.medium), strokeWidth = 2.dp)
            }
        }
    }
}

@Composable
private fun RowLine(
    text: String,
    role: ThemeText,
    secondary: Boolean,
) {
    Text(
        text = text,
        style = role.style,
        color = if (secondary) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
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
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.onboarding_saved_code_delete_title)) },
        text = { Text(body) },
        confirmButton = {
            TextButton(
                onClick = onConfirm,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text(stringResource(R.string.onboarding_saved_code_delete))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
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
                            prefix = "",
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
