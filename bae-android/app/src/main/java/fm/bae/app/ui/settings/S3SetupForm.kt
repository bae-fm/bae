package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.S3KeyFields
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeStorageFailure

/** A form field a failed setup's reason can sit under. */
private enum class S3Field { REGION, BUCKET }

/** What the person typed into the S3 form, and the reason a failed setup gave for one field. */
private class S3FormInput {
    var endpoint by mutableStateOf("")
    var region by mutableStateOf("")
    var bucket by mutableStateOf("")
    var accessKeyId by mutableStateOf("")
    var secretAccessKey by mutableStateOf("")
    var failedField by mutableStateOf<S3Field?>(null)
    var failure by mutableStateOf<DisplayError?>(null)

    val complete: Boolean
        get() = bucket.isNotEmpty() && region.isNotEmpty() && accessKeyId.isNotEmpty() && secretAccessKey.isNotEmpty()

    fun config(): BridgeSaveSyncConfig =
        BridgeSaveSyncConfig(
            bucket = bucket,
            region = region,
            endpoint = endpoint.takeIf { it.isNotEmpty() },
            accessKey = accessKeyId,
            secretKey = secretAccessKey,
            profile = null,
        )

    /**
     * Show [shown], the reason for [error], under its field when [error] names one: a region or
     * endpoint that doesn't match the bucket under Region, a bucket that isn't there or that
     * holds other files under Bucket. False for any other failure, which the screen shows.
     */
    fun place(
        error: BridgeException,
        shown: DisplayError?,
    ): Boolean {
        val failure = ((error as? BridgeException.Diagnostic)?.category as? BridgeErrorCategory.Storage)?.failure
        failedField =
            when (failure) {
                is BridgeStorageFailure.RegionMismatch -> S3Field.REGION
                is BridgeStorageFailure.ContainerNotFound, is BridgeStorageFailure.OwnLocationRequired -> S3Field.BUCKET
                else -> return false
            }
        this.failure = shown
        return true
    }

    fun clearFailure() {
        failedField = null
        failure = null
    }

    fun failureUnder(field: S3Field): String? = failure?.line?.takeIf { failedField == field }
}

/**
 * The S3 form: the endpoint, region and bucket, then the access key. coven knows a service by
 * its endpoint and fills in how to talk to it. The library takes the whole bucket. A failed
 * setup leaves the fields as typed, with the reason under the field it names. [setup] runs the
 * setup, which the person can stop with Cancel, calling [onCancel].
 */
@Composable
internal fun S3SetupForm(
    session: OpenLibrary,
    setup: StorageSetupRunner,
    onError: (DisplayError?) -> Unit,
    onCancel: () -> Unit,
) {
    val scope = rememberScreenScope()
    val input = remember { S3FormInput() }
    var busy by remember { mutableStateOf(false) }

    TextInput(
        input.endpoint,
        R.string.settings_endpoint,
        hint = R.string.storage_s3_endpoint_hint,
        keyboardType = KeyboardType.Uri,
    ) { input.endpoint = it.trim() }
    TextInput(input.region, R.string.settings_region, error = input.failureUnder(S3Field.REGION)) {
        input.region = it.trim()
    }
    TextInput(input.bucket, R.string.settings_bucket, error = input.failureUnder(S3Field.BUCKET)) {
        input.bucket = it.trim()
    }
    S3KeyFields(
        accessKeyId = input.accessKeyId,
        secretAccessKey = input.secretAccessKey,
        onAccessKeyIdChange = { input.accessKeyId = it.trim() },
        onSecretAccessKeyChange = { input.secretAccessKey = it.trim() },
    )
    StorageSetupRunLine(setup, onCancel)
    PrimaryButton(
        enabled = input.complete && !busy,
        onClick = {
            busy = true
            onError(null)
            input.clearFailure()
            scope.launch {
                setup.run(
                    "set up S3 storage",
                    claim = { input.place(it, session.errors.display(it)) },
                ) { deviceName, bridgeSetup ->
                    session.appHandle.saveSyncConfig(input.config(), deviceName, bridgeSetup)
                }
                busy = false
            }
        },
    ) {
        Text(stringResource(R.string.storage_set_up))
    }
}

@Composable
private fun TextInput(
    value: String,
    @StringRes label: Int,
    @StringRes hint: Int? = null,
    error: String? = null,
    keyboardType: KeyboardType = KeyboardType.Ascii,
    onValueChange: (String) -> Unit,
) {
    val supporting = error ?: hint?.let { stringResource(it) }
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(stringResource(label)) },
        supportingText = supporting?.let { { Text(it) } },
        isError = error != null,
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = keyboardType, autoCorrectEnabled = false),
    )
}
