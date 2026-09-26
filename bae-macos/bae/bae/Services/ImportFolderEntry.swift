import BaeKit
import Foundation

/// The one way a folder someone chose becomes something to import. The folder
/// picker, a drop on the window and Finder's Open With all come through here,
/// so what each of them checks, reports and navigates to is the same.
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
    /// else is said so rather than ignored.
    func take(_ url: URL) {
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
            return
        }
        Task {
            do {
                try await importer.addWatchedFolder(url.path)
                uiStore.navigateToImport()
            }
            catch {
                report(error)
            }
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
