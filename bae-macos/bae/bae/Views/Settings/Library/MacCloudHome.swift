import BaeKit
import SwiftUI

/// A library's cloud home in the Sync settings: the Cloud home heading over
/// the rows that say where its data is, the access key this Mac uses on S3
/// with its Replace key…, Open in browser where the provider has a folder
/// page, and under the box the line after the last key call. With `bare`, the
/// provider's rows alone, for a box that holds other rows too.
struct MacCloudHome: View {
    /// What a full cloud home adds to the provider's rows on S3.
    struct Key {
        /// The access key ID this Mac uses.
        var id: String
        /// The page's line after the last key call, until `onDismissLine`.
        var line: S3KeyReplacement.Line?
        var onReplace: () -> Void
        var onDismissLine: () -> Void
    }

    /// How much of the cloud home to draw.
    enum Shown {
        /// The provider's rows alone, for a box that holds other rows too.
        case bare
        /// The section: heading, rows, the key where `key` has one, Open in
        /// browser and the line under the box.
        case section(key: Key?)
    }

    let location: BridgeStorageLocation
    var shown = Shown.section(key: nil)

    var body: some View {
        switch shown {
        case .bare:
            CloudProviderConnectedSection(location: location)
        case .section(let key):
            MacSection {
                CloudProviderConnectedSection(location: location)
                if let key {
                    MacRow("Access key ID", value: key.id) {
                        MacButton("Replace key…", action: key.onReplace)
                    }
                }
                if let folder = location.folderLabel {
                    OpenFolderInBrowserRow(folder: folder)
                }
            } header: {
                Text("Cloud home")
            } footer: {
                if let line = key?.line {
                    S3KeyLineNotice(
                        line: line,
                        onDismiss: { key?.onDismissLine() }
                    )
                }
            }
        }
    }
}

/// `MacCloudHome` over the Sync page's key replacement and the key this Mac
/// holds. A view of its own, so the page does not redraw when the line
/// after a key call changes.
struct MacCloudHomeContainer: View {
    let location: BridgeStorageLocation
    let keyReplacement: S3KeyReplacement

    @Environment(ConfigStore.self)
    private var configStore

    var body: some View {
        MacCloudHome(
            location: location,
            shown: .section(key: key)
        )
    }

    /// The S3 access key this Mac uses, when the library is on S3 and this
    /// Mac holds an access key.
    private var key: MacCloudHome.Key? {
        guard case .s3 = location,
            case .s3AccessKey(let keyId) = configStore.config.storageAccess
        else { return nil }
        return MacCloudHome.Key(
            id: keyId,
            line: keyReplacement.line,
            onReplace: { keyReplacement.open() },
            onDismissLine: { keyReplacement.dismissLine() }
        )
    }
}

/// The page's line after an S3 key call, until the person dismisses it: a
/// saved key, with the old key to delete in the provider's console when the
/// library recorded one, or why using the key failed after its sheet closed.
private struct S3KeyLineNotice: View {
    let line: S3KeyReplacement.Line
    let onDismiss: () -> Void

    var body: some View {
        switch line {
        case .saved(let oldKey):
            MacNotice(
                title: String(localized: "Saved the new S3 key."),
                advice: oldKey.map { oldKey in
                    String(
                        localized:
                            "Delete the old key, \(oldKey), in your S3 provider’s console."
                    )
                },
                error: nil,
                tone: .info,
                onDismiss: onDismiss
            )
        case .failed(let error):
            MacNotice(
                title: String(localized: "Could not use this key"),
                advice: error.line,
                error: error,
                onDismiss: onDismiss
            )
        }
    }
}
