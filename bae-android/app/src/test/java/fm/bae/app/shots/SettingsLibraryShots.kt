package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.settings.AndroidSettingsLibrary
import fm.bae.app.ui.settings.SyncRowSummary
import fm.bae.app.ui.settings.renameLibraryParts
import org.junit.Test
import uniffi.bae_bridge.BridgeRatingSystem
import uniffi.bae_bridge.BridgeRatingSystems

/** The shots of the ready android.settings.library* boards. */
class SettingsLibraryShots : BoardShot() {
    /** The Library page as an admin sees it: the name has its edit mark and the Ratings rows open their choices. */
    @Test
    fun androidSettingsLibrary() = shot("android.settings.library") { ScreenScene { Library() } }

    /** The Library page as a member sees it: the name has no edit mark and the Ratings rows are plain values. */
    @Test
    fun androidSettingsLibraryMember() = shot("android.settings.library.member") { ScreenScene { Library(canChange = false) } }

    /** The rename dialog, with the new name typed, over the Library page before its Ratings read returns. */
    @Test
    fun androidSettingsLibraryRename() =
        shot("android.settings.library.rename") {
            InPlaceDialogScene(behind = { Library(ratings = false) }, keyboardUp = true) {
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

    /** The Library page after the rename, before its Ratings read returns. */
    @Test
    fun androidSettingsLibraryRenamed() =
        shot("android.settings.library.renamed") {
            ScreenScene { Library(name = "Our music", ratings = false) }
        }

    /** The Library page while the cloud is full, before its Ratings read returns. */
    @Test
    fun androidSettingsLibrarySyncProblem() =
        shot("android.settings.library.sync-problem") {
            ScreenScene { Library(sync = SyncRowSummary("Cloud full", StatusTone.DANGER), ratings = false) }
        }
}

@Composable
private fun Library(
    name: String = "Home music",
    canChange: Boolean = true,
    ratings: Boolean = true,
    sync: SyncRowSummary = SyncRowSummary("Synced", StatusTone.SUCCESS),
) = AndroidSettingsLibrary(
    name = name,
    canRename = canChange,
    sync = sync,
    peopleAndDevices = "2 people · 3 devices",
    hasStorage = true,
    adminChanges = null,
    onDismissChange = {},
    ratingSystems = {
        if (ratings) {
            BridgeRatingSystems(albums = BridgeRatingSystem.STARS, tracks = BridgeRatingSystem.HEART, canChange = canChange)
        } else {
            null
        }
    },
    onChangeRatingSystem = { _, _ -> },
    onBack = {},
    onRename = {},
    onOpenSync = {},
    onOpenPeople = {},
)
