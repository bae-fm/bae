package fm.bae.app.ui.components

import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * A code another device scans, one thing at a time: what to do, in [instruction]; the QR code of [qr];
 * then Copy code and Share, which hand out [code] as text and nothing else. The code's text is never
 * shown beside the QR code. [code] is not always what the QR code holds: a QR code can carry what a
 * person typing the text does not need to.
 */
@Composable
fun AndroidScanCode(
    instruction: String,
    qr: String,
    code: String,
    modifier: Modifier = Modifier,
) {
    val copy = rememberCopyText()
    val context = LocalContext.current
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
    ) {
        Text(
            text = instruction,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
        QRCodeImage(text = qr, contentDescription = stringResource(R.string.qr_code))
        Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = { copy(code) }) {
                Text(stringResource(R.string.copy_code))
            }
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = { context.shareText(code) }) {
                Text(stringResource(R.string.invite_share))
            }
        }
    }
}

/** Hand [text] to the app the person picks: a message, an email. */
private fun Context.shareText(text: String) {
    val send =
        Intent(Intent.ACTION_SEND).apply {
            type = "text/plain"
            putExtra(Intent.EXTRA_TEXT, text)
        }
    startActivity(Intent.createChooser(send, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
}
