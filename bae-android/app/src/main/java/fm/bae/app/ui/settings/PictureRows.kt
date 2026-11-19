package fm.bae.app.ui.settings

import android.content.Context
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.RowIcon
import fm.bae.app.ui.components.face
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgePersonProfile
import java.io.IOException

/**
 * The Picture row, which opens the system's photo picker, and, while the person has a picture, the Remove picture row
 * under it. Both draw the current picture and do nothing until the write of a chosen picture returns.
 */
@Composable
internal fun PictureRows(
    you: YouActions,
    profile: BridgePersonProfile,
) {
    val scope = rememberScreenScope()
    val context = LocalContext.current
    var saving by remember { mutableStateOf(false) }

    fun save(read: suspend () -> ByteArray?) {
        saving = true
        scope.launch {
            try {
                you.setAvatar(read)
            } finally {
                saving = false
            }
        }
    }
    val picker =
        rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
            if (uri != null) save { readPicked(context, uri) }
        }
    AndroidRow(
        title = stringResource(R.string.people_picture),
        avatar = profile.face(),
        icon = RowIcon.EDIT,
        enabled = !saving,
        onClick = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
    )
    if (profile.avatarVersion != null) {
        AndroidRow(
            title = stringResource(R.string.people_picture_remove),
            destructive = true,
            enabled = !saving,
            onClick = { save { null } },
        )
    }
}

/** The bytes of the image the person picked, read off the main thread. */
private suspend fun readPicked(
    context: Context,
    uri: Uri,
): ByteArray =
    withContext(Dispatchers.IO) {
        val stream = context.contentResolver.openInputStream(uri) ?: throw IOException("no file opens for $uri")
        stream.use { it.readBytes() }
    }
