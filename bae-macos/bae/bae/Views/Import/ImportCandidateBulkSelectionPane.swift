import BaeKit
import SwiftUI

struct ImportCandidateBulkSelectionPane: View {
    let selectedCount: Int
    let skipAction: ImportCandidateSkipAction

    var body: some View {
        VStack(spacing: 16) {
            Text("\(selectedCount) selected")
                .font(.title2.weight(.semibold))
            Button("Skip All") {
                Task { await skipAction.perform() }
            }
            .buttonStyle(PrimaryButtonStyle())
            .disabled(!skipAction.isEnabled)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
