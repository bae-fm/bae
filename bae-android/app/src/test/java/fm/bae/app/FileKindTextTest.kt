package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeFileKind

/** What kind of file a release file is, in the person's language. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class FileKindTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    @Test
    fun anAudioOrImageFileNamesItsFormat() {
        assertEquals("FLAC audio", context.fileKindText(BridgeFileKind.Audio("FLAC")))
        assertEquals("JPEG image", context.fileKindText(BridgeFileKind.Image("JPEG")))
    }

    @Test
    fun theOtherKindsHaveAWordEach() {
        assertEquals("Cue sheet", context.fileKindText(BridgeFileKind.CueSheet))
        assertEquals("Rip log", context.fileKindText(BridgeFileKind.RipLog))
        assertEquals("PDF document", context.fileKindText(BridgeFileKind.Pdf))
        assertEquals("File", context.fileKindText(BridgeFileKind.Other))
    }
}
