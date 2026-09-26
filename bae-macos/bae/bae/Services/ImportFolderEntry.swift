import BaeKit
import Foundation

/// The one way a folder someone chose becomes something to import. The folder
/// picker (from the menu, the import tab or the empty library), a drop on the
/// window and Finder's Open With all come through here, so what each of them
/// checks, reports and navigates to is the same.
///
/// Core reads the folder before it answers — adding it, or reading again the
/// watched folder that already covers it — and says where its releases stand.
/// This only shows that answer: the album when the library already has all of
/// it, the waiting candidates when it does not.
@MainActor
struct ImportFolderEntry {
    let importer: Importer
    let uiStore: UiStore

    /// What the folder picker handed back: a folder, or why there is none.
    func take(_ result: Result<URL, any Error>) {
        switch result {
        case .success(let url):
            take(url)
        case .failure(let error):
            report(error)
        }
    }

    /// Take in the folder at `url`. Only a folder can be imported; anything
    /// else is said so rather than ignored. The window says the folder is
    /// being read until core has answered.
    @discardableResult
    func take(_ url: URL) -> Task<Void, Never>? {
        var isDirectory: ObjCBool = false
        guard
            FileManager.default.fileExists(
                atPath: url.path,
                isDirectory: &isDirectory
            ),
            isDirectory.boolValue
        else {
            uiStore.showError(
                String(localized: "Choose a folder to import, not a file")
            )
            return nil
        }
        let reading = uiStore.beginReadingFolder(named: url.lastPathComponent)
        return Task {
            defer { uiStore.endReadingFolder(reading) }
            do {
                show(try await importer.chooseFolder(url.path))
            }
            catch {
                report(error)
            }
        }
    }

    private func show(_ chosen: BridgeChosenFolder) {
        switch chosen {
        case .inLibrary(let albumId):
            uiStore.navigateToAlbum(albumId)
        case .inImportQueue(let candidateKeys):
            guard let first = candidateKeys.first else {
                assertionFailure("core names no candidate for a waiting folder")
                uiStore.navigateToImport()
                return
            }
            uiStore.navigateToImportCandidate(
                first,
                selecting: Set(candidateKeys)
            )
        case .noReleases:
            uiStore.navigateToImport()
        }
    }

    /// `DisplayError` is nil when core says the failure has no line — a
    /// cancellation — and there is then no alert to raise. Passing the typed
    /// failure rather than a formatted `String` is what keeps the fault line
    /// and Copy Details in the alert; `addingContext` puts the operation in
    /// front of core's line without discarding either.
    private func report(_ error: any Error) {
        guard let displayed = DisplayError(error) else { return }
        uiStore.showError(
            displayed.addingContext(String(localized: "Couldn't add folder"))
        )
    }
}
