package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.StatusChip
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeReleaseRecord

/**
 * Shows a chip for each catalog that describes the release, in the order core lists them; a tap opens
 * the catalog's page for the release. [catalogName] returns the name the chip shows; a test, which has
 * no native library, passes its own.
 */
@Composable
internal fun AndroidRecordLinks(
    records: List<BridgeReleaseRecord>,
    catalogName: (BridgeCatalog) -> String,
) {
    val uriHandler = LocalUriHandler.current
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        records.forEach { record ->
            StatusChip(
                text = "${catalogName(record.catalog)} ↗",
                modifier = Modifier.clickable(role = Role.Button) { uriHandler.openUri(record.url) },
            )
        }
    }
}
