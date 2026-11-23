package fm.bae.app.shots

import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import fm.bae.app.ui.components.AndroidStorageUsage
import fm.bae.app.ui.components.StorageUsageSegment
import org.junit.Test
import uniffi.bae_bridge.BridgeStorageKind

/** The component shots of `AndroidStorageUsage`, one for each row of its preview. */
class AndroidStorageUsageShots : BoardShot() {
    @Test
    fun androidComponentStorageUsageUsual() =
        component("android.component.storage-usage.usual") {
            Usage(
                "8.3 GB",
                segment(BridgeStorageKind.PINNED, "Pinned", "6.1 GB", 0.73f),
                segment(BridgeStorageKind.CACHE, "Cache", "1.8 GB", 0.22f),
                segment(BridgeStorageKind.LIBRARY, "Library", "420 MB", 0.05f),
            )
        }

    @Test
    fun androidComponentStorageUsageFromBeforeAReset() =
        component("android.component.storage-usage.from-before-a-reset") {
            Usage(
                "9.3 GB",
                segment(BridgeStorageKind.PINNED, "Pinned", "6.1 GB", 0.66f),
                segment(BridgeStorageKind.CACHE, "Cache", "1.8 GB", 0.19f),
                segment(BridgeStorageKind.LIBRARY, "Library", "420 MB", 0.05f),
                segment(BridgeStorageKind.BEFORE_RESET, "From before the reset", "960 MB", 0.10f),
            )
        }

    @Test
    fun androidComponentStorageUsageNothingPinned() =
        component("android.component.storage-usage.nothing-pinned") {
            Usage(
                "640 MB",
                segment(BridgeStorageKind.CACHE, "Cache", "220 MB", 0.34f),
                segment(BridgeStorageKind.LIBRARY, "Library", "420 MB", 0.66f),
            )
        }
}

@Composable
private fun Usage(
    total: String,
    vararg segments: StorageUsageSegment,
) = AndroidStorageUsage(
    total = total,
    of = "on this phone",
    segments = segments.toList(),
    // The settings page scrolls. Give the legend the same height constraints inside the preview cell.
    modifier = Modifier.wrapContentHeight(align = Alignment.Top, unbounded = true),
)

private fun segment(
    kind: BridgeStorageKind,
    label: String,
    size: String,
    share: Float,
) = StorageUsageSegment(kind, label, size, share)
