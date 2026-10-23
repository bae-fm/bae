import SwiftUI

/// The Storage Manager window's content. A `View` so Observation tracks its
/// `appService` read and it re-renders once the library opens.
struct StorageManagerWindowRoot: View {
    let appDelegate: AppDelegate

    var body: some View {
        if let appService = appDelegate.appService {
            appService.installEnvironment(StorageManagerView())
        }
        else {
            ContentUnavailableView {
                Text("No library loaded")
            } description: {
                Text("Open a library first")
            }
            .frame(
                width: NoLibraryPlaceholder.width,
                height: NoLibraryPlaceholder.height
            )
        }
    }
}

/// A window's "No library loaded" placeholder.
enum NoLibraryPlaceholder {
    static let width: CGFloat = 300
    static let height: CGFloat = 200
}
