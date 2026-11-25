package fm.bae.app.shots

import fm.bae.app.ui.components.StatusChip
import org.junit.Test

/**
 * The component shot of `StatusChip`: its preview's Android row. The tone rows draw the Mac's chip, in SF Pro, so
 * Android takes no shot of them.
 */
class StatusChipShots : BoardShot() {
    @Test
    fun androidComponentStatusChipAndroid() = component("android.component.status-chip.android") { StatusChip("Synced") }
}
