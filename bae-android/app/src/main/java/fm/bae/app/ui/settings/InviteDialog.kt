package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonAlign
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.InviteDialog")

/**
 * Invite a person: the access they reach the storage with, an S3 access key
 * or the provider account they sign in with. They join as a member, and the
 * invite lasts a week. The person types their own name when they join.
 * [onCreated] receives the new invite's id, for the people screen to show it.
 */
@Composable
internal fun InviteDialog(
    session: OpenLibrary,
    storage: BridgeStorageLocation?,
    onCreated: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val scope = rememberScreenScope()
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var busy by remember { mutableStateOf(false) }
    val errors = session.errors
    InviteFormDialog(
        storage = storage,
        error = error,
        busy = busy,
        onCreate = { access ->
            busy = true
            error = null
            scope.launch {
                performBridgeAction(logger, "invite a person", errors, { error = it }) {
                    onCreated(session.appHandle.invitePerson(access))
                }
                busy = false
            }
        },
        onDismiss = onDismiss,
    )
}

/** What the person typed for the invited person's access, as the storage asks for it. */
internal class InviteAccessInput(
    email: String = "",
    accessKeyId: String = "",
    secretAccessKey: String = "",
) {
    var email by mutableStateOf(email)
    var accessKeyId by mutableStateOf(accessKeyId)
    var secretAccessKey by mutableStateOf(secretAccessKey)

    /** Whether the invited person uses this device's own S3 key, instead of one made for them. */
    var usesOwnKey by mutableStateOf(false)

    /** The access for [storage], or null while what it needs is incomplete. */
    fun access(storage: BridgeStorageLocation): BridgeInviteAccess? =
        if (storage is BridgeStorageLocation.S3) {
            when {
                usesOwnKey -> BridgeInviteAccess.HeldS3AccessKey
                accessKeyId.isEmpty() || secretAccessKey.isEmpty() -> null
                else -> BridgeInviteAccess.S3AccessKey(accessKeyId, secretAccessKey)
            }
        } else {
            email.takeIf { it.isNotEmpty() }?.let { BridgeInviteAccess.ProviderAccount(it) }
        }
}

/** The form's fields: the storage's access, and any [error]. */
@Composable
private fun InviteFormFields(
    storage: BridgeStorageLocation?,
    input: InviteAccessInput,
    error: DisplayError?,
) {
    Column(
        modifier = Modifier.verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        // Where the library is stored is a local read; its fields show once it lands.
        storage?.let { InviteAccessFields(it, input) }
        error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
    }
}

/** The invite's access, asked for as the library's storage needs it. */
@Composable
internal fun InviteFormDialog(
    storage: BridgeStorageLocation?,
    error: DisplayError?,
    busy: Boolean,
    onCreate: (BridgeInviteAccess) -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts =
            inviteFormParts(storage = storage, error = error, busy = busy, onCreate = onCreate, onDismiss = onDismiss),
    )
}

/** The form's parts; [input] holds what has been typed, which a shot hands over already filled. */
@Composable
internal fun inviteFormParts(
    storage: BridgeStorageLocation?,
    error: DisplayError?,
    busy: Boolean,
    onCreate: (BridgeInviteAccess) -> Unit,
    onDismiss: () -> Unit,
    input: InviteAccessInput = remember { InviteAccessInput() },
): DialogParts =
    DialogParts(
        title = stringResource(R.string.invite_person_title),
        content = { InviteFormFields(storage = storage, input = input, error = error) },
        confirmButton = {
            val access = storage?.let(input::access)
            AndroidButton(
                kind = ButtonKind.TEXT,
                enabled = access != null && !busy,
                onClick = { access?.let(onCreate) },
            ) {
                Text(stringResource(R.string.invite_create))
            }
        },
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = ButtonTone.NEUTRAL,
                onClick = onDismiss,
            ) { Text(stringResource(R.string.cancel)) }
        },
    )

/**
 * The fields for the invited person's access to [storage]: the provider
 * account they sign in with, or on S3 a key made for them, or this device's own.
 */
@Composable
private fun InviteAccessFields(
    storage: BridgeStorageLocation,
    input: InviteAccessInput,
) {
    // The first field takes the focus as the form opens.
    val first = remember { FocusRequester() }
    LaunchedEffect(storage is BridgeStorageLocation.S3) { if (!input.usesOwnKey) first.requestFocus() }
    if (storage is BridgeStorageLocation.S3) {
        S3InviteFields(input, first)
    } else {
        OutlinedTextField(
            value = input.email,
            onValueChange = { input.email = it.trim() },
            label = { Text(stringResource(storage.emailLabel())) },
            supportingText = { Text(stringResource(storage.emailHelp())) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().focusRequester(first),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, autoCorrectEnabled = false),
        )
    }
}

/** What the invited person's address is called, by the provider whose account it is. */
private fun BridgeStorageLocation.emailLabel(): Int =
    when (this) {
        is BridgeStorageLocation.OneDrive -> R.string.invite_email_onedrive
        is BridgeStorageLocation.CloudKit -> R.string.invite_email_icloud
        else -> R.string.invite_email_dropbox
    }

/** Why the library is shared with that account. */
private fun BridgeStorageLocation.emailHelp(): Int =
    when (this) {
        is BridgeStorageLocation.OneDrive -> R.string.invite_email_help_onedrive
        is BridgeStorageLocation.CloudKit -> R.string.invite_email_help_icloud
        else -> R.string.invite_email_help_dropbox
    }

/** An S3 invite: a key made for the invited person, or this device's own, which they then share. */
@Composable
private fun S3InviteFields(
    input: InviteAccessInput,
    first: FocusRequester,
) {
    if (input.usesOwnKey) {
        Text(
            text = stringResource(R.string.invite_s3_mine),
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        AndroidButton(kind = ButtonKind.TEXT, alignTo = ButtonAlign.TEXT, onClick = { input.usesOwnKey = false }) {
            Text(stringResource(R.string.invite_s3_use_their_key))
        }
    } else {
        S3KeyFields(
            accessKeyId = input.accessKeyId,
            secretAccessKey = input.secretAccessKey,
            onAccessKeyIdChange = { input.accessKeyId = it.trim() },
            onSecretAccessKeyChange = { input.secretAccessKey = it.trim() },
            secretNote = stringResource(R.string.invite_s3_theirs_note),
            accessKeyIdModifier = Modifier.focusRequester(first),
        )
        AndroidButton(kind = ButtonKind.TEXT, alignTo = ButtonAlign.FIELD, onClick = { input.usesOwnKey = true }) {
            Text(stringResource(R.string.invite_s3_use_my_key))
        }
    }
}
