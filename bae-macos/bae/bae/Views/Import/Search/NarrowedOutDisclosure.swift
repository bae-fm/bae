import BaeKit
import SwiftUI

/// The releases agreement left out, behind a line that says how many.
///
/// Agreement is what makes the match list short. The folder's signals agree
/// with each other — a disc ID that named three releases and a barcode that
/// named two settle on the one they share — and the folder's own text agrees
/// with what came back, so a barcode lookup that returned somebody else's
/// record ends up here too. Each of these was a real answer, and one of them
/// may be the disc on the desk, so they are here rather than gone. Closed to
/// begin with, because the matches are the answer. Open, a row set aside
/// shows on its album's card among the matches, and an album none of whose
/// rows is offered is a card below this line — picked the same way.
struct NarrowedOutDisclosure: View {
    let narrowedOut: NarrowedOut
    @Binding
    var isExpanded: Bool
    /// Library status and badges per release, keyed by release id — the
    /// state's own maps, which carry the rows set aside too.
    let libraryStatuses: [String: BridgeLibraryStatus]
    let agreements: [String: BridgeAgreements]
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
                    Text("\(narrowedOut.count) more releases")
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
                        showsNarrowedOut: true,
                        isImporting: isImporting,
                        libraryStatuses: libraryStatuses,
                        agreements: agreements,
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
