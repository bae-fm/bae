package fm.bae.app.ui

import android.Manifest
import android.content.pm.PackageManager
import android.util.Log
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.RestoreFromCodeOperation
import uniffi.bae_bridge.decodeRestoreCode
import uniffi.bae_bridge.restoreFromCodeOperation

private const val TAG = "bae.OnboardingScreen"

private class LinkFlow(
    val job: Job,
) {
    var restoreOperation: RestoreFromCodeOperation? = null

    fun cancel() {
        restoreOperation?.cancel()
        job.cancel()
    }
}

@Composable
fun OnboardingScreen(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    onLinked: (BridgeLibrary) -> Unit,
) {
    var showScanner by remember { mutableStateOf(false) }
    var showPasteDialog by remember { mutableStateOf(false) }
    var pasteInput by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var linkFlow by remember { mutableStateOf<LinkFlow?>(null) }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    fun cancelCurrentLink() {
        linkFlow?.cancel()
        linkFlow = null
    }

    // Decode the restore code, run the OAuth flow when the provider needs one
    // (Google Drive et al.), then restore. CloudKit / S3 need no token.
    val link: (String) -> Unit = { code ->
        error = null
        cancelCurrentLink()
        lateinit var flow: LinkFlow
        val launched =
            scope.launch(start = CoroutineStart.LAZY) {
                try {
                    val info = decodeRestoreCode(code)
                    val oauthTokenJson =
                        if (info.needsOauth) {
                            if (oauthLinkingError != null) {
                                throw IllegalStateException(oauthLinkingError)
                            }
                            val linking =
                                oauthLinking
                                    ?: throw IllegalStateException(
                                        context.getString(R.string.onboarding_oauth_unconfigured),
                                    )
                            linking.authorize(context, info.cloudProvider)
                        } else {
                            null
                        }
                    val operation =
                        restoreFromCodeOperation(
                            code = code,
                            oauthTokenJson = oauthTokenJson,
                        )
                    flow.restoreOperation = operation
                    val libraryInfo =
                        withContext(Dispatchers.IO) {
                            operation.restore()
                        }
                    onLinked(libraryInfo)
                } catch (e: BridgeException.Cancelled) {
                    Log.d(TAG, "link flow cancelled by bridge")
                } catch (e: CancellationException) {
                    Log.d(TAG, "link flow coroutine cancelled")
                } catch (e: Exception) {
                    error = e.toString()
                } finally {
                    if (linkFlow === flow) {
                        linkFlow = null
                    }
                }
            }
        flow = LinkFlow(launched)
        linkFlow = flow
        launched.start()
    }

    val cameraPermissionLauncher =
        rememberLauncherForActivityResult(
            ActivityResultContracts.RequestPermission(),
        ) { granted ->
            if (granted) {
                showScanner = true
            } else {
                error = context.getString(R.string.onboarding_camera_permission_required)
            }
        }

    if (showScanner) {
        QRScannerScreen(
            onScanned = { code ->
                showScanner = false
                link(code)
            },
            onDismiss = { showScanner = false },
        )
    } else if (linkFlow != null) {
        LinkingScreen(onCancel = ::cancelCurrentLink)
    } else {
        OnboardingContainer {
            Spacer(modifier = Modifier.weight(1f))

            Image(
                painter = painterResource(R.drawable.sheep_icon),
                contentDescription = stringResource(R.string.onboarding_icon_description),
                modifier = Modifier.size(120.dp),
            )

            Spacer(modifier = Modifier.height(8.dp))

            Text(
                text = "bae",
                fontSize = 48.sp,
                fontWeight = FontWeight.Bold,
            )

            Spacer(modifier = Modifier.height(12.dp))

            Text(
                text = stringResource(R.string.onboarding_tagline),
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )

            Spacer(modifier = Modifier.height(32.dp))

            val buttonWidth = Modifier.width(200.dp)

            Button(
                onClick = {
                    error = null
                    val hasPerm =
                        ContextCompat.checkSelfPermission(
                            context,
                            Manifest.permission.CAMERA,
                        ) == PackageManager.PERMISSION_GRANTED
                    if (hasPerm) {
                        showScanner = true
                    } else {
                        cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
                    }
                },
                modifier = buttonWidth,
            ) {
                Text(stringResource(R.string.onboarding_scan_qr))
            }

            Spacer(modifier = Modifier.height(8.dp))

            Button(
                onClick = {
                    error = null
                    pasteInput = ""
                    showPasteDialog = true
                },
                modifier = buttonWidth,
            ) {
                Text(stringResource(R.string.onboarding_paste_code))
            }

            if (error != null) {
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = error!!,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                )
            }

            Spacer(modifier = Modifier.weight(1f))
        }

        if (showPasteDialog) {
            AlertDialog(
                onDismissRequest = { showPasteDialog = false },
                title = { Text(stringResource(R.string.onboarding_paste_code)) },
                text = {
                    Column {
                        Text(
                            text = stringResource(R.string.onboarding_paste_code_instructions),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(modifier = Modifier.height(12.dp))
                        OutlinedTextField(
                            value = pasteInput,
                            onValueChange = { pasteInput = it },
                            placeholder = { Text(stringResource(R.string.onboarding_restore_code_placeholder)) },
                            modifier = Modifier.fillMaxWidth(),
                            textStyle =
                                MaterialTheme.typography.bodyMedium.copy(
                                    fontFamily = FontFamily.Monospace,
                                ),
                            keyboardOptions =
                                KeyboardOptions(
                                    keyboardType = KeyboardType.Ascii,
                                    autoCorrectEnabled = false,
                                ),
                            singleLine = false,
                            maxLines = 3,
                        )
                    }
                },
                confirmButton = {
                    TextButton(
                        onClick = {
                            val code = pasteInput.trim()
                            showPasteDialog = false
                            link(code)
                        },
                        enabled = pasteInput.trim().isNotEmpty(),
                    ) {
                        Text(stringResource(R.string.onboarding_connect))
                    }
                },
                dismissButton = {
                    TextButton(onClick = { showPasteDialog = false }) {
                        Text(stringResource(R.string.cancel))
                    }
                },
            )
        }
    }
}

@Composable
private fun LinkingScreen(onCancel: () -> Unit) {
    OnboardingContainer {
        CircularProgressIndicator()
        Spacer(modifier = Modifier.height(24.dp))
        Text(
            text = stringResource(R.string.onboarding_connecting_title),
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(
            text = stringResource(R.string.onboarding_connecting_body),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Spacer(modifier = Modifier.height(24.dp))
        OutlinedButton(onClick = onCancel) {
            Text(stringResource(R.string.cancel))
        }
    }
}

@Composable
private fun OnboardingContainer(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        content = content,
    )
}
