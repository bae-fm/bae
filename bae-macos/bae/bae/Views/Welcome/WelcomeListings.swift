import BaeKit
import Observation
import os.log

private let logger = Logger.bae("WelcomeListings")

/// The welcome's two lists: the libraries on this Mac, and the restore codes
/// saved in iCloud Keychain for libraries not on it. Core leaves out the code
/// of a library this Mac holds, so the lists change together: removing a
/// library from this Mac offers its code, which stays saved until the person
/// leaves the library.
@MainActor
@Observable
final class WelcomeListings {
    /// Libraries on this Mac.
    private(set) var localLibraries: SectionLoad<[BridgeLibrary]> = .loading
    /// Restore codes saved in iCloud Keychain for libraries not on this Mac.
    private(set) var savedCodes: SectionLoad<[SavedRestoreEntry]> = .loading

    /// Read both lists.
    func load(from setup: LibrarySetup) async {
        async let libraries: Void = loadLocalLibraries(from: setup)
        async let codes: Void = loadSavedRestoreCodes(from: setup)
        _ = await (libraries, codes)
    }

    /// Remove a library that isn't open, once the person confirmed `loss`,
    /// then read both lists again.
    func remove(
        closedLibrary libraryId: String,
        losing loss: RemovalLoss,
        with removals: LibraryRemovals,
        from setup: LibrarySetup
    ) async {
        await withCheckedContinuation { finished in
            removals.remove(closedLibrary: libraryId, losing: loss) {
                finished.resume()
            }
        }
        await load(from: setup)
    }

    private func loadLocalLibraries(from setup: LibrarySetup) async {
        do {
            let discovered = try await setup.listLibraries()
            try Task.checkCancellation()
            localLibraries = .loaded(discovered)
        }
        catch is CancellationError {
            logger.debug("Local library discovery cancelled")
        }
        catch {
            // A failure, not an empty list, so it doesn't read as first run.
            // No line means core reported a cancellation.
            guard let failure = DisplayError(error) else {
                logger.debug("Local library discovery cancelled")
                return
            }
            logger.error(
                "Local library discovery failed: \(error.localizedDescription)"
            )
            localLibraries = .failed(failure)
        }
    }

    private func loadSavedRestoreCodes(from setup: LibrarySetup) async {
        do {
            let saved = try await setup.savedRestoreCodes()
            try Task.checkCancellation()
            savedCodes = .loaded(
                saved.map { SavedRestoreEntry(code: $0) }
            )
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
                "Saved restore code lookup failed: \(error.localizedDescription)"
            )
            savedCodes = .failed(failure)
        }
    }
}

/// A welcome-screen lookup's state; a failure is kept apart from an empty
/// result because they show different screens.
enum SectionLoad<Value> {
    case loading
    case loaded(Value)
    case failed(DisplayError)

    var value: Value? {
        if case .loaded(let value) = self { return value }
        return nil
    }

    var failure: DisplayError? {
        if case .failed(let failure) = self { return failure }
        return nil
    }
}
