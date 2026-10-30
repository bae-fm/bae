import Observation
import os.log

private let logger = Logger.bae("SavedRestoreCodes")

/// The restore codes coven saved in iCloud Keychain for libraries not on this
/// device, including ones bae can't read, and deleting one the person no
/// longer wants offered. Deleting a code removes it from iCloud Keychain on
/// all the person's Apple devices; the library, its storage and every
/// device's copy of it stay.
@MainActor
@Observable
public final class SavedRestoreCodes {
    /// The saved codes, or why they couldn't be read.
    public private(set) var list: SectionLoad<[SavedRestoreEntry]> = .loading
    /// The code the person is asked to confirm deleting.
    public private(set) var pendingDelete: SavedRestoreEntry?
    /// The library whose code is being deleted.
    public private(set) var deletingLibraryId: String?
    /// Why the last delete failed.
    public private(set) var deleteError: DisplayError?

    public init() {}

    #if DEBUG
        /// Codes already read, for previews, with the delete confirmation
        /// open for `confirming`.
        public init(
            previewing entries: [SavedRestoreEntry],
            confirming: SavedRestoreEntry? = nil
        ) {
            list = .loaded(entries)
            pendingDelete = confirming
        }
    #endif

    /// Read the saved codes.
    public func load(from setup: LibrarySetup) async {
        do {
            let saved = try await setup.savedRestoreCodes()
            try Task.checkCancellation()
            list = .loaded(saved.map(SavedRestoreEntry.init))
        }
        catch is CancellationError {
            logger.debug("Saved restore code lookup cancelled")
        }
        catch {
            // A refused lookup (such as a locked keychain) isn't the same as
            // no restore codes. No line means core reported a cancellation.
            guard let failure = DisplayError(error) else {
                logger.debug("Saved restore code lookup cancelled")
                return
            }
            logger.error(
                "Saved restore code lookup failed: \(String(reflecting: error))"
            )
            list = .failed(failure)
        }
    }

    /// Ask the person to confirm deleting `entry`'s code.
    public func askToDelete(_ entry: SavedRestoreEntry) {
        pendingDelete = entry
    }

    /// Close the delete confirmation without deleting.
    public func dismissDelete() {
        pendingDelete = nil
    }

    /// Delete `entry`'s code from iCloud Keychain once the person confirmed,
    /// then read the codes again, which no longer list it. A failure leaves
    /// the code listed and shows as `deleteError`; deleting again finishes a
    /// delete that stopped partway.
    public func delete(
        _ entry: SavedRestoreEntry,
        from setup: LibrarySetup
    ) async {
        pendingDelete = nil
        deleteError = nil
        deletingLibraryId = entry.libraryId
        defer { deletingLibraryId = nil }
        do {
            try await setup.forgetSavedRestoreCode(entry.libraryId)
        }
        catch {
            guard let failure = DisplayError(error) else {
                logger.debug("Saved restore code delete cancelled")
                return
            }
            logger.error(
                "Saved restore code delete failed: \(String(reflecting: error))"
            )
            deleteError = failure
            return
        }
        await load(from: setup)
    }
}
