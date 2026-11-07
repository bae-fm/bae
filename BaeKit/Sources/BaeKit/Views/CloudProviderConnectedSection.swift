import SwiftUI
import os.log

private let logger = Logger.bae("CloudProviderConnectedSection")

/// The library's storage as coven records it, one row per field.
public struct CloudProviderConnectedSection: View {
    public let location: BridgeStorageLocation

    public init(location: BridgeStorageLocation) {
        self.location = location
    }

    public var body: some View {
        LabeledContent("Provider", value: location.displayName)
        switch location {
        case .s3(let bucket, let region, let endpoint, _):
            if let endpoint {
                LabeledContent("Endpoint", value: endpoint)
            }
            LabeledContent("Region", value: region)
            LabeledContent("Bucket", value: bucket)
        case .dropbox, .oneDrive:
            if let account = location.accountLabel {
                LabeledContent("Account", value: account)
            }
            if let folder = location.folderLabel {
                FolderRows(folder: folder)
            }
        case .cloudKit:
            EmptyView()
        }
    }
}

/// A Dropbox shared folder's or a OneDrive folder's name, and Open in browser,
/// which opens the folder's page at the provider.
private struct FolderRows: View {
    let folder: BridgeFolderLabel

    @Environment(\.openURL)
    private var openURL
    /// Why the folder's page didn't open, until the next try.
    @State
    private var openFailure: DisplayError?

    var body: some View {
        LabeledContent("Folder", value: folder.name)
        HStack {
            Spacer()
            Button("Open in browser", action: open)
        }
        if let openFailure {
            ErrorDetailDisclosure(error: openFailure)
        }
    }

    private func open() {
        openFailure = nil
        // coven records only an https address, so one Foundation can't read
        // is a broken guarantee.
        guard let url = URL(string: folder.webUrl) else {
            let detail =
                "the folder's web address isn't a URL: \(folder.webUrl)"
            logger.error("\(detail)")
            CoreBugReporter()
                .report(
                    operation: "open the folder in the browser",
                    detail: detail
                )
            openFailure = DisplayError(
                line: localizedCoreString("core.error.category.internal")
            )
            return
        }
        openURL(url) { opened in
            guard !opened else { return }
            logger.warning("no app opened the folder's web address \(url)")
            openFailure = DisplayError(
                line: String(
                    localized: "No app on this device opened the folder's page."
                )
            )
        }
    }
}
