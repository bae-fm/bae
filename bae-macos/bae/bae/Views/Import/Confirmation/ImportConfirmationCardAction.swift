import BaeKit
import SwiftUI

/// The commit bar's trailing action: `Import`, or `Retry Import` after a
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
        if case .error = importStatus {
            Button("Retry Import") { onConfirmImport() }
                .buttonStyle(PrimaryButtonStyle())
        }
        else {
            Button("Import") { onConfirmImport() }
                .buttonStyle(PrimaryButtonStyle())
        }
    }
}

#if DEBUG
    #Preview("Card action — ready") {
        ImportConfirmationCardAction(
            importStatus: nil,
            onConfirmImport: {}
        )
        .padding()
        .windowBackground()
    }
#endif
