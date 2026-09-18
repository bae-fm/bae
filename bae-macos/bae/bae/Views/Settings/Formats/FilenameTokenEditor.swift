import BaeKit
import SwiftUI

/// The export filename pattern editor: the pattern's tokens as removable,
/// drag-reorderable chips in a field, and an "Add:" row offering the tokens the
/// pattern doesn't use yet. Every edit sends the whole new token list up —
/// the caller writes it through and the change round-trips via `configChanged`.
struct FilenameTokenEditor: View {
    let tokens: [BridgeSaveFilenameToken]
    let setTokens: ([BridgeSaveFilenameToken]) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            chipField
            if !availableTokens.isEmpty {
                addRow
            }
        }
    }

    private var chipField: some View {
        FlowLayout(spacing: 5) {
            ForEach(tokens, id: \.self) { token in
                TokenChip(token: token) {
                    setTokens(tokens.filter { $0 != token })
                }
                .draggable(token.dragId)
                .dropDestination(for: String.self) { items, _ in
                    dropToken(items, before: token)
                }
            }
        }
        .frame(maxWidth: .infinity, minHeight: 24, alignment: .leading)
        .padding(6)
        .background(
            // The same fill a text field uses, so the pattern reads as an
            // editable input next to the Name field, not a panel.
            RoundedRectangle(cornerRadius: 6)
                .fill(Color(nsColor: .textBackgroundColor))
                .stroke(.separator)
        )
        .dropDestination(for: String.self) { items, _ in
            dropToken(items, before: nil)
        }
    }

    private var addRow: some View {
        FlowLayout(spacing: 5) {
            Text("Add:")
                .font(.caption)
                .foregroundStyle(.secondary)
            ForEach(availableTokens, id: \.self) { token in
                Button(token.label) {
                    setTokens(tokens + [token])
                }
                .buttonStyle(.plain)
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 2)
                .overlay(
                    RoundedRectangle(cornerRadius: 5)
                        .stroke(
                            .tertiary,
                            style: StrokeStyle(lineWidth: 1, dash: [3, 2])
                        )
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var availableTokens: [BridgeSaveFilenameToken] {
        BridgeSaveFilenameToken.allTokens.filter { !tokens.contains($0) }
    }

    /// Reinsert a dragged chip before `target`, or at the end when the drop
    /// landed on the field background rather than a chip.
    private func dropToken(
        _ items: [String],
        before target: BridgeSaveFilenameToken?
    ) -> Bool {
        guard let id = items.first,
            let dragged = BridgeSaveFilenameToken(dragId: id),
            tokens.contains(dragged),
            dragged != target
        else { return false }
        var reordered = tokens.filter { $0 != dragged }
        if let target, let index = reordered.firstIndex(of: target) {
            reordered.insert(dragged, at: index)
        }
        else {
            reordered.append(dragged)
        }
        setTokens(reordered)
        return true
    }
}

private struct TokenChip: View {
    let token: BridgeSaveFilenameToken
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 5) {
            Text(token.label)
                .font(.callout)
            Button(action: remove) {
                Image(systemName: "xmark")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Remove \(token.label)"))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(
            RoundedRectangle(cornerRadius: 5)
                .fill(.quaternary)
        )
    }
}

extension BridgeSaveFilenameToken {
    /// Every token, in the order the "Add:" row offers them.
    static let allTokens: [Self] = [
        .trackNumber, .title, .artist, .album, .year, .discNumber, .trackTotal,
    ]

    var label: String {
        switch self {
        case .title: String(localized: "Title")
        case .artist: String(localized: "Artist")
        case .album: String(localized: "Album")
        case .year: String(localized: "Release year")
        case .trackNumber: String(localized: "Track №")
        case .discNumber: String(localized: "Disc №")
        case .trackTotal: String(localized: "Track total")
        }
    }

    /// The sample value the settings preview line substitutes for this token.
    /// The numeric samples stay literal — filenames aren't locale-formatted —
    /// and the track number mirrors the exporter's two-digit padding.
    var sampleValue: String {
        switch self {
        case .title: String(localized: "Track Title")
        case .artist: String(localized: "Artist Name")
        case .album: String(localized: "Album Title")
        case .year: "2020"
        case .trackNumber: "04"
        case .discNumber: "1"
        case .trackTotal: "12"
        }
    }

    /// Stable identity a chip drag carries; the token list itself crosses the
    /// bridge as typed enums, this string never leaves the drag session.
    var dragId: String {
        switch self {
        case .title: "title"
        case .artist: "artist"
        case .album: "album"
        case .year: "year"
        case .trackNumber: "trackNumber"
        case .discNumber: "discNumber"
        case .trackTotal: "trackTotal"
        }
    }

    init?(dragId: String) {
        guard
            let token = Self.allTokens.first(where: { $0.dragId == dragId })
        else { return nil }
        self = token
    }

    /// The sample filename the settings preview lines show for a pattern: the
    /// tokens' sample values joined with spaces (empty patterns fall back to
    /// the title, mirroring the exporter) plus the given extension.
    static func previewFilename(
        tokens: [Self],
        fileExtension: String
    ) -> String {
        let stem = tokens.map(\.sampleValue).joined(separator: " ")
        return
            "\(stem.isEmpty ? Self.title.sampleValue : stem).\(fileExtension)"
    }
}

#if DEBUG
    #Preview("Filename token editor") {
        @Previewable
        @State
        var tokens: [BridgeSaveFilenameToken] = [
            .trackNumber, .title,
        ]
        Form {
            FilenameTokenEditor(tokens: tokens, setTokens: { tokens = $0 })
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 200)
    }
#endif
