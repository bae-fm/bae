import SwiftUI

/// The Storage Manager window's content. A `View` so Observation tracks its
/// `appService` read and it re-renders once the library opens.
struct StorageManagerWindowRoot: View {
    let appDelegate: AppDelegate

    var body: some View {
        if let appService = appDelegate.appService {
            appService.installEnvironment(StorageManagerView())
                .environment(
                    \.openSettingsWindow,
                    OpenSettingsWindowAction {
                        appDelegate.showSettingsWindow(nil)
                    }
                )
        }
        else {
            StorageManagerNoLibrary()
        }
    }
}

/// The Storage Manager window with no library open: its toolbar controls,
/// each off, over the message that says why.
struct StorageManagerNoLibrary: View {
    var body: some View {
        ContentUnavailableView {
            Text("No library loaded")
        } description: {
            Text("Open a library first")
        }
        .frame(
            minWidth: StorageManagerView.minimumWidth,
            maxWidth: .infinity,
            minHeight: StorageManagerView.minimumHeight,
            maxHeight: .infinity
        )
        .storageManagerToolbar(
            filter: .constant(.all),
            search: .constant(""),
            inspectorPresented: .constant(false),
            off: true
        )
    }
}
