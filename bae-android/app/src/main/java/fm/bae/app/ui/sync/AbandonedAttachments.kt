package fm.bae.app.ui.sync

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import uniffi.bae_bridge.BridgeAbandonedAttachment
import uniffi.bae_bridge.BridgeAbandonedAttachments

/** Each upload abandoned because its row took a newer change first: what it was, why, and Dismiss. */
internal fun LazyListScope.abandonedRows(
    context: Context,
    abandoned: BridgeAbandonedAttachments,
    onDismiss: (BridgeAbandonedAttachment) -> Unit,
) {
    when (abandoned) {
        BridgeAbandonedAttachments.Loading -> {
            Unit
        }

        is BridgeAbandonedAttachments.Failed -> {
            item { FailureDisclosure(context, abandoned.error) }
        }

        is BridgeAbandonedAttachments.Ready -> {
            items(abandoned.attachments, key = { it.id }) { attachment ->
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
                    Text(text = context.coreString(attachment.subject), style = ThemeText.strong.style)
                    SecondaryLine(context.coreString(attachment.reasonKey))
                    TextButton(onClick = { onDismiss(attachment) }) {
                        Text(context.coreString("core.abandoned.action.dismiss"))
                    }
                }
            }
        }
    }
}
