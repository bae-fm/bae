package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.settings.AndroidSettingsLibrary
import fm.bae.app.ui.settings.SyncRowSummary
import fm.bae.app.ui.settings.renameLibraryParts
import org.junit.Test

/** The shots of the ready android.settings.library* boards. */
class SettingsLibraryShots : BoardShot() {
    /** The Library page as an admin sees it: the name has its edit mark. */
    @Test
    fun androidSettingsLibrary() = shot("android.settings.library") { ScreenScene { Library() } }

    /** The Library page as a member sees it: the name has no edit mark. */
    @Test
    fun androidSettingsLibraryMember() = shot("android.settings.library.member") { ScreenScene { Library(canRename = false) } }

    /** The rename dialog, with the new name typed, over the Library page. */
    @Test
    fun androidSettingsLibraryRename() =
        shot("android.settings.library.rename") {
            InPlaceDialogScene(behind = { Library() }) {
                renameLibraryParts(
                    draft = "Our music",
                    error = null,
                    saving = false,
                    onDraftChange = {},
                    onSave = {},
                    onDismiss = {},
                )
            }
        }

    /** The Library page after the rename. */
    @Test
    fun androidSettingsLibraryRenamed() = shot("android.settings.library.renamed") { ScreenScene { Library(name = "Our music") } }

    /** The Library page while the cloud is full. */
    @Test
    fun androidSettingsLibrarySyncProblem() =
        shot("android.settings.library.sync-problem") { ScreenScene { Library(sync = SyncRowSummary("Cloud full", StatusTone.DANGER)) } }
}

@Composable
private fun Library(
    name: String = "Home music",
    canRename: Boolean = true,
    sync: SyncRowSummary = SyncRowSummary("Synced", StatusTone.SUCCESS),
) = AndroidSettingsLibrary(
    name = name,
    canRename = canRename,
    sync = sync,
    peopleAndDevices = "2 people · 3 devices",
    hasStorage = true,
    onBack = {},
    onRename = {},
    onOpenSync = {},
    onOpenPeople = {},
    onOpenRestoreCode = {},
)
