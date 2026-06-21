package fm.bae.app.ui

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMembership

private const val TAG = "bae.MembersScreen"
private val logger = BaeLogger(TAG)

/**
 * Loads and holds the library's membership: its devices and whether this device
 * is the owner (the gate for inviting and removing). Reading the chain hits cloud
 * storage, so the load runs off the main thread; the screen refreshes after every
 * approve or remove so the list reflects the new membership.
 */
private class MembersModel(
    private val session: OpenLibrary,
    private val appContext: Context,
) {
    var membership by mutableStateOf<BridgeMembership?>(null)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<String?>(null)
        private set

    // A remove failure, shown above the list (the list stays visible).
    var actionError by mutableStateOf<String?>(null)
        private set

    suspend fun load() {
        loading = true
        error = null
        try {
            membership = withContext(Dispatchers.IO) { session.appHandle.getMembers() }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load members", e)
            error = e.message ?: appContext.getString(R.string.members_load_failed)
        } finally {
            loading = false
        }
    }

    suspend fun remove(member: BridgeMember) {
        actionError = null
        try {
            withContext(Dispatchers.IO) { session.appHandle.removeMember(member.pubkey) }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error(
                "Failed to remove member ${member.fingerprint}",
                e,
            )
            actionError = e.message ?: appContext.getString(R.string.members_remove_failed)
        } finally {
            load()
        }
    }
}

@Composable
fun MembersScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val appContext = LocalContext.current.applicationContext
    val scope = rememberCoroutineScope()
    val model = remember { MembersModel(session, appContext) }
    LaunchedEffect(Unit) { model.load() }

    var showAddDevice by remember { mutableStateOf(false) }
    var pendingRemoval by remember { mutableStateOf<BridgeMember?>(null) }

    Column(modifier = Modifier.fillMaxSize()) {
        MembersTopBar(onBack = onBack)
        model.actionError?.let { msg ->
            Text(
                text = msg,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )
        }
        MembersBody(
            model = model,
            onReload = { scope.launch { model.load() } },
            onAddDevice = { showAddDevice = true },
            onRemove = { pendingRemoval = it },
        )
    }

    if (showAddDevice) {
        AddDeviceSheet(
            session = session,
            onDismiss = { showAddDevice = false },
            onInvited = { scope.launch { model.load() } },
        )
    }

    pendingRemoval?.let { member ->
        RemoveDeviceDialog(
            member = member,
            onConfirm = {
                pendingRemoval = null
                scope.launch { model.remove(member) }
            },
            onDismiss = { pendingRemoval = null },
        )
    }
}

@Composable
private fun MembersBody(
    model: MembersModel,
    onReload: () -> Unit,
    onAddDevice: () -> Unit,
    onRemove: (BridgeMember) -> Unit,
) {
    val membership = model.membership
    when {
        model.loading && membership == null -> {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
        }

        model.error != null && membership == null -> {
            Column(
                modifier = Modifier.fillMaxSize().padding(32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text(text = model.error!!, color = MaterialTheme.colorScheme.error)
                TextButton(onClick = onReload) {
                    Text(stringResource(R.string.retry))
                }
            }
        }

        membership != null -> {
            MembersList(
                membership = membership,
                onAddDevice = onAddDevice,
                onRemove = onRemove,
            )
        }
    }
}

@Composable
private fun MembersTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(
                text = stringResource(R.string.members_title),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

@Composable
private fun MembersList(
    membership: BridgeMembership,
    onAddDevice: () -> Unit,
    onRemove: (BridgeMember) -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item {
            Text(
                text = stringResource(R.string.members_explanation),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        items(membership.members, key = { it.pubkey }) { member ->
            MemberRow(
                member = member,
                onRemove = { onRemove(member) },
            )
        }
        if (membership.selfIsOwner) {
            item {
                Spacer(modifier = Modifier.height(8.dp))
                Button(onClick = onAddDevice) {
                    Text(stringResource(R.string.members_add_device))
                }
            }
        }
    }
}

@Composable
private fun MemberRow(
    member: BridgeMember,
    onRemove: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    text = member.fingerprint,
                    style = MaterialTheme.typography.bodyMedium,
                    fontFamily = FontFamily.Monospace,
                )
                AssistChip(onClick = {}, enabled = false, label = { Text(stringResource(member.role.labelRes())) })
            }
            if (member.isSelf) {
                Text(
                    text = stringResource(R.string.members_this_device),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        if (member.canRemove) {
            TextButton(
                onClick = onRemove,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text(stringResource(R.string.members_remove))
            }
        }
    }
}

private fun BridgeMemberRole.labelRes(): Int =
    when (this) {
        BridgeMemberRole.OWNER -> R.string.members_role_owner
        BridgeMemberRole.MEMBER -> R.string.members_role_member
        BridgeMemberRole.FOLLOWER -> R.string.members_role_follower
    }

@Composable
private fun RemoveDeviceDialog(
    member: BridgeMember,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.members_remove_confirm_title)) },
        text = {
            Text(stringResource(R.string.members_remove_confirm_body, member.fingerprint))
        },
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.members_remove)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}
