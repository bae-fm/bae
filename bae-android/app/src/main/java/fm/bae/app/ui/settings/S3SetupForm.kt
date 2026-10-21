package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.text.input.KeyboardType
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeS3BucketVersioning
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeS3Detection
import uniffi.bae_bridge.BridgeS3Profile
import uniffi.bae_bridge.BridgeS3ProfileName
import uniffi.bae_bridge.BridgeS3Versioning
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.bridgeErrorCategoryKey

private val logger = BaeLogger("bae.S3SetupForm")

/** The S3-compatible services coven has a profile for, by their own names. */
private fun profileName(profile: BridgeS3ProfileName): String =
    when (profile) {
        BridgeS3ProfileName.AWS -> "Amazon S3"
        BridgeS3ProfileName.GCS -> "Google Cloud Storage"
        BridgeS3ProfileName.R2 -> "Cloudflare R2"
        BridgeS3ProfileName.RUST_FS -> "RustFS"
        BridgeS3ProfileName.SEAWEED_FS -> "SeaweedFS"
        BridgeS3ProfileName.CEPH_RGW -> "Ceph RGW"
        BridgeS3ProfileName.VERSITY_GW -> "Versity Gateway"
    }

/** What the person typed into the S3 form. */
private class S3FormInput {
    var bucket by mutableStateOf("")
    var region by mutableStateOf("")
    var endpoint by mutableStateOf("")
    var accessKeyId by mutableStateOf("")
    var secretAccessKey by mutableStateOf("")
    var keyPrefix by mutableStateOf("")
    var profile by mutableStateOf<BridgeS3ProfileName?>(null)

    val complete: Boolean
        get() = bucket.isNotEmpty() && region.isNotEmpty() && accessKeyId.isNotEmpty() && secretAccessKey.isNotEmpty()

    val credentials: BridgeS3Credentials
        get() = BridgeS3Credentials(accessKeyId, secretAccessKey)

    /** The bucket as a location, for coven to check before anything is set up. */
    fun location(): BridgeStorageLocation =
        BridgeStorageLocation.S3(
            bucket = bucket,
            region = region,
            endpoint = endpoint.takeIf { it.isNotEmpty() },
            profile = profile?.let { BridgeS3Profile.Named(it) },
            prefix = keyPrefix,
        )

    fun config(): BridgeSaveSyncConfig =
        BridgeSaveSyncConfig(
            bucket = bucket,
            region = region,
            endpoint = endpoint.takeIf { it.isNotEmpty() },
            keyPrefix = keyPrefix.takeIf { it.isNotEmpty() },
            accessKey = accessKeyId,
            secretKey = secretAccessKey,
            profile = profile?.let { BridgeS3Profile.Named(it) },
        )
}

/**
 * The S3 form: the bucket, its region and endpoint, and the access key. coven
 * knows a service by its endpoint and fills in how to talk to it; Advanced
 * holds the key prefix, a profile for a service coven doesn't know, and a
 * check of what coven detects in the bucket.
 */
@Composable
internal fun S3SetupForm(
    session: OpenLibrary,
    onError: (DisplayError?) -> Unit,
    onSetUp: (BridgeStorageSetup) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val input = remember { S3FormInput() }
    var showAdvanced by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }

    TextInput(input.bucket, R.string.settings_bucket) { input.bucket = it.trim() }
    TextInput(input.region, R.string.settings_region) { input.region = it.trim() }
    TextInput(input.endpoint, R.string.settings_endpoint, R.string.storage_s3_endpoint_hint, KeyboardType.Uri) {
        input.endpoint = it.trim()
    }
    S3KeyFields(
        accessKeyId = input.accessKeyId,
        secretAccessKey = input.secretAccessKey,
        onAccessKeyIdChange = { input.accessKeyId = it.trim() },
        onSecretAccessKeyChange = { input.secretAccessKey = it.trim() },
    )
    TextButton(onClick = { showAdvanced = !showAdvanced }) { Text(stringResource(R.string.storage_s3_advanced)) }
    if (showAdvanced) {
        S3Advanced(session = session, input = input, onError = onError)
    }
    PrimaryButton(
        enabled = input.complete && !busy,
        onClick = {
            busy = true
            onError(null)
            scope.launch {
                runSetup(context, "set up S3 storage", onError, onSetUp) { deviceName ->
                    session.appHandle.saveSyncConfig(input.config(), deviceName)
                }
                busy = false
            }
        },
    ) {
        Text(stringResource(R.string.storage_set_up))
    }
}

/** The key prefix, the service profile, and a check of the bucket as coven finds it. */
@Composable
private fun S3Advanced(
    session: OpenLibrary,
    input: S3FormInput,
    onError: (DisplayError?) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var detection by remember { mutableStateOf<BridgeS3Detection?>(null) }
    var checking by remember { mutableStateOf(false) }
    TextInput(input.keyPrefix, R.string.storage_s3_key_prefix) { input.keyPrefix = it.trim() }
    ProfileChoice(input.profile) { input.profile = it }
    OutlinedButton(
        enabled = input.complete && !checking,
        onClick = {
            checking = true
            onError(null)
            scope.launch {
                performBridgeAction(logger, "detect S3 settings", LocaleErrorDisplays(context), onError) {
                    detection = session.appHandle.detectS3Settings(input.location(), input.credentials)
                }
                checking = false
            }
        },
    ) {
        Text(stringResource(R.string.storage_s3_check))
    }
    detection?.let { DetectionResults(it) }
}

@Composable
private fun TextInput(
    value: String,
    @StringRes label: Int,
    @StringRes hint: Int? = null,
    keyboardType: KeyboardType = KeyboardType.Ascii,
    onValueChange: (String) -> Unit,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(stringResource(label)) },
        supportingText = hint?.let { { Text(stringResource(it)) } },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType, autoCorrectEnabled = false),
    )
}

/** Which service's profile coven uses: the one it detects from the endpoint, or one chosen here. */
@Composable
private fun ProfileChoice(
    profile: BridgeS3ProfileName?,
    onChoose: (BridgeS3ProfileName?) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val detect = stringResource(R.string.storage_s3_profile_detect)
    Text(stringResource(R.string.storage_s3_profile), style = ThemeText.detail.style)
    OutlinedButton(onClick = { open = true }) {
        Text(profile?.let(::profileName) ?: detect)
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        DropdownMenuItem(text = { Text(detect) }, onClick = {
            open = false
            onChoose(null)
        })
        BridgeS3ProfileName.entries.forEach { name ->
            DropdownMenuItem(text = { Text(profileName(name)) }, onClick = {
                open = false
                onChoose(name)
            })
        }
    }
}

/** What coven found in the bucket: whether it keeps versions, and the settings and probes behind Details. */
@Composable
private fun DetectionResults(detection: BridgeS3Detection) {
    val context = LocalContext.current
    var showDetails by remember(detection) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(
            when (val versioning = detection.versioning()) {
                is BridgeS3Versioning.Known -> {
                    stringResource(
                        when (versioning.state) {
                            BridgeS3BucketVersioning.DISABLED -> R.string.storage_versioning_disabled
                            BridgeS3BucketVersioning.ENABLED -> R.string.storage_versioning_enabled
                            BridgeS3BucketVersioning.SUSPENDED -> R.string.storage_versioning_suspended
                            BridgeS3BucketVersioning.UNAVAILABLE -> R.string.storage_versioning_unavailable
                        },
                    )
                }

                is BridgeS3Versioning.Failed -> {
                    val category = BridgeErrorCategory.S3VersioningUnknown(versioning.failure)
                    context.coreString(bridgeErrorCategoryKey(category))
                }
            },
        )
        TextButton(onClick = { showDetails = !showDetails }) { Text(stringResource(R.string.details)) }
        if (showDetails) {
            SelectionContainer {
                Text(
                    text = detectionDetails(detection),
                    style = ThemeText.mono.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** The detected settings and each probe's outcome, as coven reports them. */
private fun detectionDetails(detection: BridgeS3Detection): String =
    buildString {
        appendLine(detection.settings().toString())
        detection.results().forEach { result ->
            appendLine("${if (result.held) "✓" else "✗"} ${result.probe}: ${result.detail}")
        }
    }.trimEnd()
