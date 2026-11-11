package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.NoErrorDisplays
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.LibraryEnding
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.RemoveLibraryDialog
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeLibraryStanding
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStandingState
import uniffi.bae_bridge.BridgeStorageLocation
import java.time.Instant

private fun library(
    id: String,
    name: String,
    state: BridgeLibraryState,
) = BridgeLibrary(id = id, name = name, path = "/libraries/$id", state = state, isActive = false)

private val synced =
    library(
        "synced",
        "Home Library",
        BridgeLibraryState.Openable(
            BridgeStorageLocation.Dropbox(BridgeDropboxLocation.AppFolder, accountLabel = "ana@example.com", folderLabel = null),
        ),
    )
private val neverSynced = library("local", "Local Library", BridgeLibraryState.Openable(storage = null))
private val removalStopped = library("stopped", "Half Removed", BridgeLibraryState.DeletionUnfinished)

/** What the welcome's library core was asked, in order, and what it answers. */
private class FakeLibraries(
    var libraries: List<BridgeLibrary>,
) {
    val calls = mutableListOf<String>()
    var unsent = false
    var failure: BridgeException? = null
    var standing = BridgeLibraryStanding(BridgeStandingState.UnsentChanges, lastSynced = null)
    var standingFailure: BridgeException? = null

    fun model(scope: CoroutineScope) =
        WelcomeLibraries(
            scope,
            core(),
            errorDisplay = { DisplayError("Storage refused") },
            onFilesRemain = {},
        )

    fun core() =
        WelcomeLibrariesCore(
            listLibraries = {
                calls += "list"
                libraries
            },
            libraryStanding = { libraryId ->
                calls += "standing $libraryId"
                standingFailure?.let { throw it }
                standing
            },
            unsentBeforeRemoval = { libraryId, _ ->
                calls += "unsent $libraryId"
                unsent
            },
            removeOpened = { libraryId, _ ->
                calls += "remove $libraryId"
                failure?.let { throw it }
                libraries = libraries.filter { it.id != libraryId }
                BridgeLibraryRemoval.Removed
            },
            removeUnopenable = { libraryId, _ ->
                calls += "remove unopenable $libraryId"
                failure?.let { throw it }
                libraries = libraries.filter { it.id != libraryId }
                BridgeLibraryRemoval.Removed
            },
        )
}

/** The welcome lists this device's libraries: Remove, or Delete for one that was never synced, asks first. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class WelcomeLibrariesTest {
    @get:Rule
    val compose = createComposeRule()

    private fun showLibraries(fake: FakeLibraries) {
        compose.setContent {
            val scope = rememberCoroutineScope()
            val model = remember { fake.model(scope) }
            BaeTheme {
                WelcomeLibrariesSection(model, onOpen = { fake.calls += "open ${it.id}" }, onAdd = { fake.calls += "add" })
            }
        }
    }

    /** The whole welcome over [fake]'s libraries, as onboarding shows it. */
    private fun showWelcome(fake: FakeLibraries) {
        compose.setContent {
            val context = LocalContext.current
            val scope = rememberCoroutineScope()
            val flow =
                remember {
                    OnboardingFlow(
                        scope,
                        context,
                        OnboardingCore(
                            NoJoinsHost(),
                            NoErrorDisplays::display,
                            ::FakeBootstrapCancellation,
                            ::unfakedOpenProgress,
                            ::fakeSetupStatus,
                        ),
                        null,
                        null,
                    ) {}
                }
            val model = remember { fake.model(scope) }
            BaeTheme {
                OnboardingSteps(
                    flow,
                    WelcomeLibrariesView(listed = fake.libraries.isNotEmpty()) { onAdd ->
                        WelcomeLibrariesSection(model, onOpen = {}, onAdd = onAdd)
                    },
                )
            }
        }
    }

    /** Open the only library's ⋮ menu and choose [item]. */
    private fun chooseFromMenu(item: String) {
        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText(item).performClick()
    }

    @Test
    fun aLibraryThatSyncsIsRemovedAndOneThatDoesntIsDeleted() {
        assertEquals(LibraryEnding.REMOVE, LibraryEnding.of(synced.state))
        assertEquals(LibraryEnding.DELETE, LibraryEnding.of(neverSynced.state))
        assertEquals(LibraryEnding.REMOVE, LibraryEnding.of(removalStopped.state))
    }

    @Test
    fun theListNamesEachLibraryAndNothingElse() {
        showLibraries(FakeLibraries(listOf(synced, neverSynced)))

        compose.onNodeWithText("Your libraries").assertIsDisplayed()
        compose.onNodeWithText("Home Library").assertIsDisplayed()
        compose.onNodeWithText("Local Library").assertIsDisplayed()
        compose.onNodeWithText("Dropbox: ana@example.com").assertDoesNotExist()
        compose.onNodeWithText("Not synced").assertDoesNotExist()
        compose.onAllNodesWithContentDescription("More").assertCountEquals(2)
    }

    /** The row being counted shows what it waits on as words under its name, a spinner where the ⋮ was, and nothing else taps. */
    @Test
    fun aLibraryBeingCountedShowsWhatItWaitsOnInsteadOfItsMenu() {
        val fake = FakeLibraries(listOf(synced, neverSynced))
        compose.setContent {
            BaeTheme {
                WelcomeLibrariesContent(
                    list = WelcomeLibrariesList.Loaded(fake.libraries),
                    running = RunningRemoval(synced.id, removing = false, opening = null, closing = null),
                    callbacks =
                        WelcomeLibrariesCallbacks(
                            onOpen = { fake.calls += "open ${it.id}" },
                            onDetails = {},
                            onRemove = {},
                            onAdd = { fake.calls += "add" },
                        ),
                )
            }
        }

        compose.onNodeWithText("Home Library").assertIsDisplayed()
        compose.onNodeWithText("Opening the library…").assertIsDisplayed()
        compose.onAllNodesWithContentDescription("More").assertCountEquals(1)

        compose.onNodeWithText("Local Library").performClick()
        compose.onNodeWithText("Add library").performClick()

        assertEquals(emptyList<String>(), fake.calls)
    }

    @Test
    fun addLibraryIsTheListsLastRow() {
        val fake = FakeLibraries(listOf(synced, neverSynced))
        showLibraries(fake)

        compose.onNodeWithText("Add library").assertIsDisplayed()
        compose.onNodeWithText("Add library").performClick()

        assertEquals(listOf("list", "add"), fake.calls)
    }

    @Test
    fun addLibraryOpensTheSheetWithScanAndEnterCode() {
        showWelcome(FakeLibraries(listOf(synced)))

        compose.onNodeWithText("Scan code").assertDoesNotExist()
        compose.onNodeWithText("Add library").performClick()

        compose.onNodeWithText("Scan code").assertIsDisplayed()
        compose.onNodeWithText("Enter code").assertIsDisplayed()
    }

    @Test
    fun removeWarnsOfUnsentChangesThenAsksBeforeLeavingIt() {
        val fake = FakeLibraries(listOf(synced))
        fake.unsent = true
        showLibraries(fake)

        chooseFromMenu("Remove…")

        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("unsent synced") }
        compose.onNodeWithText("Remove “Home Library” from this device?").assertIsDisplayed()
        compose.onNodeWithText("Unsent changes will be lost").assertIsDisplayed()
        compose.onNodeWithText("The library will remain in the cloud and on your other devices.").assertIsDisplayed()
        assertEquals(listOf("list", "unsent synced"), fake.calls)

        compose.onNode(hasText("Remove") and hasAnyAncestor(isDialog())).performClick()

        compose.waitUntil(HANG_GUARD_MS) { fake.calls.last() == "list" && fake.calls.contains("remove synced") }
        compose.onNodeWithText("Home Library").assertDoesNotExist()
        assertEquals(listOf("list", "unsent synced", "remove synced", "list"), fake.calls)
    }

    @Test
    fun nothingUnsentShowsNoWarning() {
        val fake = FakeLibraries(listOf(synced))
        showLibraries(fake)

        chooseFromMenu("Remove…")

        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("unsent synced") }
        compose.onNodeWithText("Remove “Home Library” from this device?").assertIsDisplayed()
        compose.onNodeWithText("The library will remain in the cloud and on your other devices.").assertIsDisplayed()
        compose.onNodeWithText("Unsent changes will be lost").assertDoesNotExist()
    }

    @Test
    fun deleteAsksForANeverSyncedLibraryAndSaysItIsGoneForGood() {
        val fake = FakeLibraries(listOf(neverSynced))
        showLibraries(fake)

        chooseFromMenu("Delete…")

        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("unsent local") }
        compose.onNodeWithText("Delete “Local Library”?").assertIsDisplayed()
        compose
            .onNodeWithText("This library isn't synced, so it will be deleted for good. Your audio files stay in their folders.")
            .assertIsDisplayed()

        compose.onNode(hasText("Delete") and hasAnyAncestor(isDialog())).performClick()

        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("remove local") }
        compose.waitUntil(HANG_GUARD_MS) { fake.libraries.isEmpty() }
    }

    /** A library that can't open has no count to read, so asking goes straight to the confirmation. */
    @Test
    fun aLibraryThatCantOpenIsNotOpenedToCountButRemovedWithoutOpening() =
        runBlocking {
            val fake = FakeLibraries(listOf(removalStopped))
            val model = fake.model(CoroutineScope(Dispatchers.Unconfined))

            model.askToRemove(removalStopped)

            assertEquals(PendingRemoval(removalStopped, RemovalLoss.Uncounted), model.pending)
            assertEquals(emptyList<String>(), fake.calls)

            model.remove(model.pending!!)

            assertEquals(listOf("remove unopenable stopped", "list"), fake.calls)
            assertEquals(null, model.pending)
        }

    @Test
    fun theConfirmationForALibraryThatCantOpenSaysItsUnsentChangesAreLost() {
        compose.setContent {
            BaeTheme {
                RemoveLibraryDialog(
                    library = removalStopped,
                    loss = RemovalLoss.Uncounted,
                    onConfirm = {},
                    onDismiss = {},
                )
            }
        }

        compose.onNodeWithText("Remove “Half Removed” from this device?").assertIsDisplayed()
        compose.onNodeWithText("Unsent changes will be lost").assertDoesNotExist()
        compose
            .onNodeWithText(
                "This library can't be opened, so any changes on this device that weren't sent are lost. " +
                    "Its copy on this device is deleted; if the library is kept in the cloud, it stays there.",
            ).assertIsDisplayed()
    }

    @Test
    fun cancellingTheConfirmationChangesNothing() {
        val fake = FakeLibraries(listOf(neverSynced))
        showLibraries(fake)

        chooseFromMenu("Delete…")
        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("unsent local") }
        compose.onNode(hasText("Cancel") and hasAnyAncestor(isDialog())).performClick()

        compose.onNodeWithText("Local Library").assertIsDisplayed()
        assertEquals(listOf("list", "unsent local"), fake.calls)
    }

    @Test
    fun aFailedRemovalIsADialogAndRetryRunsItAgain() {
        val fake = FakeLibraries(listOf(synced))
        fake.failure = BridgeException.Diagnostic(BridgeErrorCategory.Keyring, "refused")
        showLibraries(fake)

        chooseFromMenu("Remove…")
        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("unsent synced") }
        compose.onNode(hasText("Remove") and hasAnyAncestor(isDialog())).performClick()

        compose.waitUntil(HANG_GUARD_MS) { compose.onAllNodesWithTextCount("Retry") == 1 }
        compose.onNodeWithText("Couldn’t remove “Home Library”").assertIsDisplayed()
        compose.onNodeWithText("Storage refused").assertIsDisplayed()

        fake.failure = null
        compose.onNodeWithText("Retry").performClick()

        compose.waitUntil(HANG_GUARD_MS) { fake.libraries.isEmpty() }
        assertEquals(listOf("list", "unsent synced", "remove synced", "remove synced", "list"), fake.calls)
    }

    @Test
    fun cancellingAFailedRemovalLeavesTheRowAsItWas() {
        val fake = FakeLibraries(listOf(synced))
        fake.failure = BridgeException.Diagnostic(BridgeErrorCategory.Keyring, "refused")
        showLibraries(fake)

        chooseFromMenu("Remove…")
        compose.waitUntil(HANG_GUARD_MS) { fake.calls.contains("unsent synced") }
        compose.onNode(hasText("Remove") and hasAnyAncestor(isDialog())).performClick()
        compose.waitUntil(HANG_GUARD_MS) { compose.onAllNodesWithTextCount("Retry") == 1 }

        compose.onNodeWithText("Cancel").performClick()

        compose.onNodeWithText("Retry").assertDoesNotExist()
        compose.onNodeWithText("Home Library").assertIsDisplayed()
        compose.onNodeWithContentDescription("More").assertIsDisplayed()
        assertEquals(listOf("list", "unsent synced", "remove synced"), fake.calls)
    }

    @Test
    fun detailsShowWhatTheReadReturned() =
        runBlocking {
            val fake = FakeLibraries(listOf(synced))
            val moment = Instant.now()
            fake.standing = BridgeLibraryStanding(BridgeStandingState.UnsentChanges, moment)
            val model = fake.model(CoroutineScope(Dispatchers.Unconfined))

            model.showDetails(synced)

            assertEquals(listOf("standing synced"), fake.calls)
            assertEquals(LibraryDetails(synced, StandingRead.Loaded(fake.standing)), model.details)

            model.closeDetails()

            assertEquals(null, model.details)
        }

    @Test
    fun aLibraryThatNeverSyncedHasNothingToRead() =
        runBlocking {
            val fake = FakeLibraries(listOf(neverSynced))
            val model = fake.model(CoroutineScope(Dispatchers.Unconfined))

            model.showDetails(neverSynced)

            assertEquals(emptyList<String>(), fake.calls)
            assertEquals(LibraryDetails(neverSynced, standing = null), model.details)
        }

    @Test
    fun aFailedReadShowsItsErrorInTheSheet() =
        runBlocking {
            val fake = FakeLibraries(listOf(synced))
            fake.standingFailure = BridgeException.Diagnostic(BridgeErrorCategory.Keyring, "refused")
            val model = fake.model(CoroutineScope(Dispatchers.Unconfined))

            model.showDetails(synced)

            assertEquals(
                LibraryDetails(synced, StandingRead.Failed(DisplayError("Storage refused"))),
                model.details,
            )
        }

    @Test
    fun theSheetNamesTheLibraryItsStatusAndWhenItLastSynced() {
        val fake = FakeLibraries(listOf(synced))
        fake.standing = BridgeLibraryStanding(BridgeStandingState.UnsentChanges, Instant.now().minusSeconds(7_200))
        showLibraries(fake)

        compose.onNodeWithContentDescription("More").performClick()
        compose.onNodeWithText("Details…").performClick()

        compose.waitUntil(HANG_GUARD_MS) { compose.onAllNodes(hasText("Unsent changes")).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText("Status").assertIsDisplayed()
        compose.onNodeWithText("Last synced").assertIsDisplayed()
        // The storage rows sit below the sheet's fold on the test screen.
        compose.onNodeWithText("Provider").assertExists()
    }

    @Test
    fun tappingALibraryOpensIt() {
        val fake = FakeLibraries(listOf(synced))
        showLibraries(fake)

        compose.onNodeWithText("Home Library").performClick()

        assertEquals(listOf("list", "open synced"), fake.calls)
    }
}

private fun androidx.compose.ui.test.junit4.ComposeContentTestRule.onAllNodesWithTextCount(text: String): Int =
    onAllNodes(hasText(text)).fetchSemanticsNodes().size
