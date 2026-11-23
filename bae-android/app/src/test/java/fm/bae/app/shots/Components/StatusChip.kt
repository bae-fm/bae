package fm.bae.app.shots

import fm.bae.app.ui.components.StatusChip
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `StatusChip`, one for each row of its preview. */
class StatusChipShots : BoardShot() {
    @Test
    fun androidComponentStatusChipNeutral() = component("android.component.status-chip.neutral") { StatusChip("Synced") }

    @Test
    fun androidComponentStatusChipAccent() =
        component("android.component.status-chip.accent") { StatusChip("Synced", tone = StatusTone.INFO) }

    @Test
    fun androidComponentStatusChipInfo() = component("android.component.status-chip.info") { StatusChip("Synced", tone = StatusTone.INFO) }

    @Test
    fun androidComponentStatusChipSuccess() =
        component("android.component.status-chip.success") { StatusChip("Synced", tone = StatusTone.SUCCESS) }

    @Test
    fun androidComponentStatusChipWarning() =
        component("android.component.status-chip.warning") { StatusChip("Synced", tone = StatusTone.WARNING) }

    @Test
    fun androidComponentStatusChipDanger() =
        component("android.component.status-chip.danger") { StatusChip("Synced", tone = StatusTone.DANGER) }
}
