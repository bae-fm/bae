import BaeKit
import Observation
import os.log

private let logger = Logger.bae("WelcomeListings")

/// The welcome's two lists: the libraries on this Mac, and the restore codes
/// saved in iCloud Keychain for libraries not on it. Core leaves out the code
/// of a library this Mac holds, so the lists change together: removing a
/// library from this Mac offers its code, which stays saved until the person
/// leaves the library or deletes the code.
@MainActor
@Observable
final class WelcomeListings {
    /// Libraries on this Mac.
    private(set) var localLibraries: SectionLoad<[BridgeLibrary]> = .loading
    /// Restore codes saved in iCloud Keychain for libraries not on this Mac.
    let savedCodes = SavedRestoreCodes()

    /// Read both lists.
    func load(from setup: LibrarySetup) async {
        async let libraries: Void = loadLocalLibraries(from: setup)
        async let codes: Void = savedCodes.load(from: setup)
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
                "Local library discovery failed: \(String(reflecting: error))"
            )
            localLibraries = .failed(failure)
        }
    }
}
