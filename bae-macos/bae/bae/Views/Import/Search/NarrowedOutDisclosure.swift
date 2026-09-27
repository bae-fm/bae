import BaeKit
import SwiftUI

/// The releases agreement left out, closed behind a line that says how many;
/// each was a real answer and can still be picked.
struct NarrowedOutDisclosure: View {
    let narrowedOut: NarrowedOut
    @Binding
    var isExpanded: Bool
    /// Library status and badges per release, keyed by release id.
    let libraryStatuses: [String: BridgeLibraryStatus]
    let agreements: [String: BridgeAgreements]
    let isImporting: Bool
    let selectedReleaseId: String?
    let loadingReleaseId: String?
    var releaseSelectionFailure: ReleaseSelectionFailure?
    /// Identify the candidate again, reading once more the documents a run
    /// could not; `nil` where no run read any, as for a typed search.
    var onRetryUnread: (() -> Void)?
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
                    Text("\(Int(narrowedOut.count)) more releases")
                }
                .themeText(.body)
                .foregroundStyle(.secondary)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            if isExpanded {
                ForEach(narrowedOut.groups) { group in
                    ReleaseGroupSection(
                        group: group,
                        showsNarrowedOut: true,
                        isImporting: isImporting,
                        libraryStatuses: libraryStatuses,
                        agreements: agreements,
                        selectedReleaseId: selectedReleaseId,
                        loadingReleaseId: loadingReleaseId,
                        releaseSelectionFailure: releaseSelectionFailure,
                        onRetryUnread: onRetryUnread,
                        onSelect: onSelect,
                    )
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
