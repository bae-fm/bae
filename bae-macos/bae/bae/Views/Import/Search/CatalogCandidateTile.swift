import BaeKit
import SwiftUI

/// A catalog number extraction found and the run is not looking up: where it
/// was read, and the number, dimmed. Clicking it promotes it to a row of the
/// ledger and both providers look it up.
struct CatalogCandidateTile: View {
    let candidate: BridgeCatalogCandidate
    let filePaths: [String: String]
    let onActivate: () -> Void

    @State
    private var isHovered = false

    var body: some View {
        Button(action: onActivate) {
            HStack(spacing: 6) {
                ForEach(Array(candidate.sources.enumerated()), id: \.offset) {
                    _,
                    source in
                    SignalSourceChip(source: source, filePaths: filePaths)
                        .opacity(0.5)
                }
                Text(candidate.value)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
            .padding(.leading, 6)
            .padding(.trailing, 9)
            .padding(.vertical, 4)
            .background(
                Color.primary.opacity(isHovered ? 0.04 : 0),
                in: RoundedRectangle(cornerRadius: 6)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(
                        Color.primary.opacity(isHovered ? 0.18 : 0.09),
                        lineWidth: 1
                    )
            )
            .contentShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
        .help("Look this catalog number up")
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Catalog tiles") {
        FlowLayout(spacing: 6) {
            ForEach(PreviewData.catalogCandidates, id: \.value) { candidate in
                CatalogCandidateTile(
                    candidate: candidate,
                    filePaths: [:],
                    onActivate: {}
                )
            }
        }
        .padding()
        .frame(width: 400)
        .windowBackground()
    }
#endif
