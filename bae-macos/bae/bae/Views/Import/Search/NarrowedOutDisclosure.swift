import BaeKit
import SwiftUI

/// The releases agreement left out, behind a line that says how many.
///
/// Agreement between a folder's signals is what makes the match list short: a
/// disc ID that named three releases and a barcode that named two settle on
/// the one they share. The other four were real answers, and one of them may
/// be the disc on the desk — so they are here rather than gone. Closed to
/// begin with, because the matches are the answer; open, they are the same
/// cards as the matches, picked the same way.
struct NarrowedOutDisclosure: View {
    let narrowedOut: NarrowedOut
    @Binding
    var isExpanded: Bool
    let isImporting: Bool
    let selectedReleaseId: String?
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    let onSelect: (Pressing) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Button {
                isExpanded.toggle()
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    Text("\(narrowedOut.pressingCount) more releases")
                }
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            if isExpanded {
                ForEach(narrowedOut.groups) { group in
                    ReleaseGroupSection(
                        group: group,
                        isImporting: isImporting,
                        libraryStatuses: narrowedOut.libraryStatuses,
                        provenance: narrowedOut.provenance,
                        selectedReleaseId: selectedReleaseId,
                        loadingReleaseId: loadingReleaseId,
                        releaseSelectionFailure: releaseSelectionFailure,
                        onSelect: onSelect,
                    )
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
