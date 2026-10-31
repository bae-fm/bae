package fm.bae.app.ui.onboarding

import android.app.Application
import android.os.Looper
import fm.bae.app.DisplayError
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeException

/** The welcome's saved codes are read from coven, and a delete removes one from this phone and reads them again. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SavedRestoreCodesTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    @After
    fun endWork() {
        scope.cancel()
    }

    private fun codesOn(host: SavedCodesHost): SavedRestoreCodes {
        val codes = SavedRestoreCodes(scope, host) { DisplayError((it as BridgeException.Diagnostic).detail) }
        scope.launch { codes.load() }
        shadowOf(Looper.getMainLooper()).idle()
        return codes
    }

    private val SavedRestoreCodes.libraryIds: List<String>
        get() = (list as SavedCodesList.Loaded).entries.map { it.libraryId }

    @Test
    fun theListShowsEveryEntryIncludingOnesBaeCantRead() {
        val codes = codesOn(SavedCodesHost(listOf(dropboxCode), unreadable = listOf(UNREADABLE_LIBRARY)))

        val entries = (codes.list as SavedCodesList.Loaded).entries
        assertEquals("Home Library", (entries[0] as SavedRestoreEntry.Readable).saved.info.name)
        assertEquals(UNREADABLE_LIBRARY, (entries[1] as SavedRestoreEntry.Unreadable).libraryId)
    }

    @Test
    fun aRefusedReadShowsWhyRatherThanNoCodes() {
        val host = SavedCodesHost(listOf(dropboxCode)).apply { refuseReads = true }

        val codes = codesOn(host)

        assertEquals(SavedCodesList.Failed(DisplayError("Block Store refused the read")), codes.list)
    }

    @Test
    fun deletingACodeRemovesItFromThisPhoneAndReadsTheListAgain() {
        val host = SavedCodesHost(listOf(dropboxCode, s3Code(hasS3Credentials = true)))
        val codes = codesOn(host)
        val dropbox = (codes.list as SavedCodesList.Loaded).entries.first()

        codes.askToDelete(dropbox)
        assertEquals(dropbox, codes.pendingDelete)
        codes.delete(dropbox)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("saved-dropbox"), host.forgotten)
        assertEquals(listOf("saved-s3"), codes.libraryIds)
        assertNull(codes.pendingDelete)
        assertNull(codes.deletingLibraryId)
    }

    @Test
    fun anEntryBaeCantReadIsDeletedByItsLibraryId() {
        val host = SavedCodesHost(emptyList(), unreadable = listOf(UNREADABLE_LIBRARY))
        val codes = codesOn(host)

        codes.delete((codes.list as SavedCodesList.Loaded).entries.single())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf(UNREADABLE_LIBRARY), host.forgotten)
        assertEquals(emptyList<String>(), codes.libraryIds)
    }

    @Test
    fun aRefusedDeleteKeepsTheCodeListedAndSaysWhy() {
        val host = SavedCodesHost(listOf(dropboxCode)).apply { refuseDeletes = true }
        val codes = codesOn(host)

        codes.delete((codes.list as SavedCodesList.Loaded).entries.single())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("saved-dropbox"), codes.libraryIds)
        assertEquals(DisplayError("Block Store refused the delete"), codes.deleteError)
        assertNull(codes.deletingLibraryId)
    }
}
