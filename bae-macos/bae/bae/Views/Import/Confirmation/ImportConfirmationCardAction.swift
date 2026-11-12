import BaeKit
import SwiftUI

/// The commit bar's trailing action: `Import`, or `Retry import` after a
/// failed attempt. The bar is shown only before an import runs or after one
/// fails; a running or completed import has panes of its own.
///
/// `Import` is never disabled; an edit core cannot save is refused at commit
/// and the pane states why.
struct ImportConfirmationCardAction: View {
    /// What the candidate's last import left, as its row places it.
    let importStatus: BridgeCandidateImportStatus?
    let onConfirmImport: () -> Void

    var body: some View {
        Button(Self.title(importStatus)) { onConfirmImport() }
            .buttonStyle(PrimaryButtonStyle())
    }

    /// The action's words: Retry import once an attempt failed, else Import.
    static func title(_ importStatus: BridgeCandidateImportStatus?) -> String {
        if case .error = importStatus {
            return String(localized: "Retry import")
        }
        return String(localized: "Import")
    }
}
