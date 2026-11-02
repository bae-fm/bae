package fm.bae.app.ui.components

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext

/**
 * Puts text on the clipboard, from a button's click. It sets Android's clipboard
 * directly: Compose's LocalClipboard only sets it from a coroutine, which a click
 * has no need of.
 */
@Composable
internal fun rememberCopyText(): (String) -> Unit {
    val context = LocalContext.current
    return remember(context) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        val copy: (String) -> Unit = { text -> clipboard.setPrimaryClip(ClipData.newPlainText(null, text)) }
        copy
    }
}
