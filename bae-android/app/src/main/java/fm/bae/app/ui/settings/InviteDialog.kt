package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteLifetime
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.InviteDialog")

/**
 * Invite a person: their role and the access they reach the storage with —
 * an S3 access key, or the provider account they sign in with — and how long
 * the invite lasts. The person types their own name when they join.
 * [onCreated] receives the new
 * invite's id, for the people screen to show it.
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
        onCreate = { form ->
            busy = true
            error = null
            scope.launch {
                performBridgeAction(logger, "invite a person", errors, { error = it }) {
                    onCreated(session.appHandle.invitePerson(form.role, form.access, form.lifetime))
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

    /** The access for [storage], or null while what it needs is incomplete. */
    fun access(storage: BridgeStorageLocation): BridgeInviteAccess? =
        if (storage is BridgeStorageLocation.S3) {
            if (accessKeyId.isEmpty() || secretAccessKey.isEmpty()) {
                null
            } else {
                BridgeInviteAccess.S3AccessKey(accessKeyId, secretAccessKey)
            }
        } else {
            email.takeIf { it.isNotEmpty() }?.let { BridgeInviteAccess.ProviderAccount(it) }
        }
}

/** A complete invite form: with what role and access, and how long it lasts. */
internal class InviteForm(
    val role: BridgeMemberRole,
    val access: BridgeInviteAccess,
    val lifetime: BridgeInviteLifetime,
)

/** The role and lifetime the form asks for. */
private class InviteFields {
    var role by mutableStateOf(BridgeMemberRole.MEMBER)
    var lifetime by mutableStateOf(BridgeInviteLifetime.DAY)
}

/** The form's fields: role, the storage's access, and how long the invite lasts. */
@Composable
private fun InviteFormFields(
    storage: BridgeStorageLocation?,
    fields: InviteFields,
    input: InviteAccessInput,
    error: DisplayError?,
) {
    Column(
        modifier = Modifier.verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        RoleChoice(fields.role) { fields.role = it }
        // Where the library is stored is a local read; its fields show once it lands.
        storage?.let { InviteAccessFields(it, input) }
        Choice(
            choices = BridgeInviteLifetime.entries,
            chosen = fields.lifetime,
            label = { choice ->
                when (choice) {
                    BridgeInviteLifetime.HOUR -> R.string.invite_lifetime_hour
                    BridgeInviteLifetime.DAY -> R.string.invite_lifetime_day
                    BridgeInviteLifetime.WEEK -> R.string.invite_lifetime_week
                }
            },
            onChoose = { fields.lifetime = it },
        )
        error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
    }
}

/** The invite's role, access and lifetime, asked for as the library's storage needs it. */
@Composable
internal fun InviteFormDialog(
    storage: BridgeStorageLocation?,
    error: DisplayError?,
    busy: Boolean,
    onCreate: (InviteForm) -> Unit,
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
    onCreate: (InviteForm) -> Unit,
    onDismiss: () -> Unit,
    input: InviteAccessInput = remember { InviteAccessInput() },
): DialogParts {
    val fields = remember { InviteFields() }
    return DialogParts(
        title = stringResource(R.string.members_invite),
        content = {
            InviteFormFields(
                storage = storage,
                fields = fields,
                input = input,
                error = error,
            )
        },
        confirmButton = {
            val access = storage?.let(input::access)
            AndroidButton(
                kind = ButtonKind.TEXT,
                enabled = access != null && !busy,
                onClick = { access?.let { onCreate(InviteForm(fields.role, it, fields.lifetime)) } },
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
}

/** The fields for the invited person's access to [storage]: an S3 access key, or their provider account. */
@Composable
private fun InviteAccessFields(
    storage: BridgeStorageLocation,
    input: InviteAccessInput,
) {
    if (storage is BridgeStorageLocation.S3) {
        S3KeyFields(
            accessKeyId = input.accessKeyId,
            secretAccessKey = input.secretAccessKey,
            onAccessKeyIdChange = { input.accessKeyId = it.trim() },
            onSecretAccessKeyChange = { input.secretAccessKey = it.trim() },
        )
    } else {
        OutlinedTextField(
            value = input.email,
            onValueChange = { input.email = it.trim() },
            label = { Text(stringResource(R.string.invite_email)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, autoCorrectEnabled = false),
        )
    }
}

@Composable
private fun RoleChoice(
    role: BridgeMemberRole,
    onChoose: (BridgeMemberRole) -> Unit,
) {
    Choice(
        choices = listOf(BridgeMemberRole.MEMBER, BridgeMemberRole.ADMIN),
        chosen = role,
        label = { choice ->
            when (choice) {
                BridgeMemberRole.MEMBER -> R.string.members_role_member
                BridgeMemberRole.ADMIN -> R.string.members_role_admin
            }
        },
        onChoose = onChoose,
    )
}

/** One of a few [choices] as a row of segmented buttons, each named by its [label] string. */
@Composable
private fun <T> Choice(
    choices: List<T>,
    chosen: T,
    label: (T) -> Int,
    onChoose: (T) -> Unit,
) {
    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
        choices.forEachIndexed { index, choice ->
            SegmentedButton(
                selected = chosen == choice,
                onClick = { onChoose(choice) },
                shape = SegmentedButtonDefaults.itemShape(index, choices.size),
            ) {
                Text(stringResource(label(choice)))
            }
        }
    }
}
