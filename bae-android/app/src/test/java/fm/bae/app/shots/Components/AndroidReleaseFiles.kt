package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AndroidReleaseFiles
import org.junit.Test
import uniffi.bae_bridge.BridgeFileKind

/** The component shots of `AndroidReleaseFiles`, one for each row of its preview. */
class AndroidReleaseFilesShots : BoardShot() {
    @Test
    fun androidComponentReleaseFilesThreeFiles() =
        component("android.component.release-files.three-files", wide = true) {
            AndroidReleaseFiles(
                listOf(
                    boardFile("Low Tide Hours.cue", BridgeFileKind.CueSheet),
                    boardFile("Low Tide Hours.log", BridgeFileKind.RipLog),
                    boardFile("booklet.pdf", BridgeFileKind.Pdf),
                ),
            )
        }

    @Test
    fun androidComponentReleaseFilesOneFile() =
        component("android.component.release-files.one-file", wide = true) {
            AndroidReleaseFiles(listOf(boardFile("Low Tide Hours.log", BridgeFileKind.RipLog)))
        }

    @Test
    fun androidComponentReleaseFilesLongName() =
        component("android.component.release-files.long-name", wide = true) {
            AndroidReleaseFiles(
                listOf(boardFile("Songs for the Slow Orchards, Recorded Live at the Harbor Hall.cue", BridgeFileKind.CueSheet)),
            )
        }
}
