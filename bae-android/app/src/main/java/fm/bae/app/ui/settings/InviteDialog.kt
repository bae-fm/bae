package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteDelivery
import uniffi.bae_bridge.BridgeInviteLifetime
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeStorageLocation

private val logger = BaeLogger("bae.InviteDialog")

/**
 * Invite a person: their name, role and the access they reach the storage
 * with — an S3 access key, or the provider account they sign in with — and
 * whether the invite is shown in person or sent. [onCreated] receives the new
 * invite's id, for the people screen to show it.
 */
@Composable
internal fun InviteDialog(
    session: OpenLibrary,
    storage: BridgeStorageLocation?,
    onCreated: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var busy by remember { mutableStateOf(false) }
    val errors = LocaleErrorDisplays(context)
    InviteFormDialog(
        storage = storage,
        error = error,
        busy = busy,
        onCreate = { form ->
            busy = true
            error = null
            scope.launch {
                performBridgeAction(logger, "invite a person", errors, { error = it }) {
                    val invite = session.appHandle.invitePerson(form.role, form.name, form.access, form.delivery)
                    onCreated(invite.id)
                }
                busy = false
            }
        },
        onDismiss = onDismiss,
    )
}

/** What the person typed for the invited person's access, as the storage asks for it. */
private class InviteAccessInput {
    var email by mutableStateOf("")
    var accessKeyId by mutableStateOf("")
    var secretAccessKey by mutableStateOf("")

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

/** A complete invite form: who, with what role and access, and how it reaches them. */
private class InviteForm(
    val name: String,
    val role: BridgeMemberRole,
    val access: BridgeInviteAccess,
    val delivery: BridgeInviteDelivery,
)

/** How the invite reaches the person, as the form chooses it. */
private enum class DeliveryChoice { IN_PERSON, SEND }

/** The person, role and delivery the form asks for. */
private class InviteFields {
    var name by mutableStateOf("")
    var role by mutableStateOf(BridgeMemberRole.MEMBER)
    var delivery by mutableStateOf(DeliveryChoice.IN_PERSON)
    var lifetime by mutableStateOf(BridgeInviteLifetime.DAY)

    fun delivery(): BridgeInviteDelivery =
        when (delivery) {
            DeliveryChoice.IN_PERSON -> BridgeInviteDelivery.InPerson
            DeliveryChoice.SEND -> BridgeInviteDelivery.Sent(lifetime)
        }
}

/** The form's fields: name, role, the storage's access, and in person or sent. */
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
        OutlinedTextField(
            value = fields.name,
            onValueChange = { fields.name = it },
            label = { Text(stringResource(R.string.invite_name)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
        )
        RoleChoice(fields.role) { fields.role = it }
        if (storage == null) {
            CircularProgressIndicator()
        } else {
            InviteAccessFields(storage, input)
        }
        Choice(
            choices = DeliveryChoice.entries,
            chosen = fields.delivery,
            label = { choice ->
                when (choice) {
                    DeliveryChoice.IN_PERSON -> R.string.invite_in_person
                    DeliveryChoice.SEND -> R.string.invite_send
                }
            },
            onChoose = { fields.delivery = it },
        )
        if (fields.delivery == DeliveryChoice.SEND) {
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
        }
        error?.let { ErrorDetailDisclosure(it) }
    }
}

/** The invite's person, role, access and delivery, asked for as the library's storage needs it. */
@Composable
private fun InviteFormDialog(
    storage: BridgeStorageLocation?,
    error: DisplayError?,
    busy: Boolean,
    onCreate: (InviteForm) -> Unit,
    onDismiss: () -> Unit,
) {
    val fields = remember { InviteFields() }
    val input = remember { InviteAccessInput() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.members_invite)) },
        text = {
            InviteFormFields(
                storage = storage,
                fields = fields,
                input = input,
                error = error,
            )
        },
        confirmButton = {
            val access = storage?.let(input::access)
            val name = fields.name.trim()
            TextButton(
                enabled = access != null && name.isNotEmpty() && !busy,
                onClick = { access?.let { onCreate(InviteForm(name, fields.role, it, fields.delivery())) } },
            ) {
                Text(stringResource(R.string.invite_create))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
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
