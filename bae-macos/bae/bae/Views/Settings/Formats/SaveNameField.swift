import BaeKit
import SwiftUI

/// A piece a save name is made of, as the pane's chips offer it.
protocol SaveNameToken: Hashable {
    /// Every token, in the order the "Add:" row offers them.
    static var allTokens: [Self] { get }
    var label: String { get }
    /// This token's value in the settings preview.
    var sampleValue: String { get }
    /// The string a chip drag carries; it never leaves the drag session.
    var dragId: String { get }
}

extension SaveNameToken {
    static func token(dragId: String) -> Self? {
        allTokens.first { $0.dragId == dragId }
    }

    /// The settings preview of a pattern's name; an empty pattern shows
    /// `fallback`, as a save names by it.
    static func previewName(tokens: [Self], fallback: Self) -> String {
        let name = tokens.map(\.sampleValue).joined(separator: " ")
        return name.isEmpty ? fallback.sampleValue : name
    }
}

/// A name pattern Save as… writes with: its title above, the pattern's
/// pieces as reorderable chips, an "Add:" row of unused pieces, and a preview
/// of a name it makes.
struct SaveNameField<Token: SaveNameToken>: View {
    let title: LocalizedStringKey
    let tokens: [Token]
    let setTokens: ([Token]) -> Void
    /// A name the pattern makes, shown under the field.
    let preview: String

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(title)
                .themeText(.rowTitle)
            chipField
            if !availableTokens.isEmpty {
                addRow
            }
            Text(
                "Preview: \(Text(preview).monospaced())"
            )
            .fieldNote()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var chipField: some View {
        FlowLayout(spacing: ThemeSpace.compact) {
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
        .padding(ThemeSpace.compact)
        .background(
            // A text field's fill, so the pattern reads as an input.
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(Color(nsColor: .textBackgroundColor))
                .stroke(.separator)
        )
        .dropDestination(for: String.self) { items, _ in
            dropToken(items, before: nil)
        }
    }

    private var addRow: some View {
        FlowLayout(spacing: ThemeSpace.compact) {
            Text("Add:")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            ForEach(availableTokens, id: \.self) { token in
                Button(token.label) {
                    setTokens(tokens + [token])
                }
                .buttonStyle(.plain)
                .themeText(.chip)
                .foregroundStyle(StatusTone.neutral.color)
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.line)
                .overlay(
                    RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        .stroke(
                            .tertiary,
                            style: StrokeStyle(lineWidth: 1, dash: [3, 2])
                        )
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var availableTokens: [Token] {
        Token.allTokens.filter { !tokens.contains($0) }
    }

    /// Reinsert a dragged chip before `target`, or at the end when the drop
    /// landed on the field background rather than a chip.
    private func dropToken(_ items: [String], before target: Token?) -> Bool {
        guard let id = items.first,
            let dragged = Token.token(dragId: id),
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

private struct TokenChip<Token: SaveNameToken>: View {
    let token: Token
    let remove: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            Text(token.label)
                .themeText(.chip)
            Button(action: remove) {
                Image(systemName: "xmark")
                    .themeIcon(.badge)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Remove \(token.label)"))
        }
        .padding(.horizontal, ThemeSpace.compact)
        .padding(.vertical, ThemeSpace.line)
        .background(
            StatusTone.neutral.fill,
            in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
        )
    }
}

extension BridgeSaveFilenameToken: SaveNameToken {
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

    /// Numbers stay unlocalized, as in real filenames.
    var sampleValue: String {
        switch self {
        case .title: String(localized: "Track title")
        case .artist: String(localized: "Artist name")
        case .album: String(localized: "Album title")
        case .year: "2020"
        case .trackNumber: "04"
        case .discNumber: "1"
        case .trackTotal: "12"
        }
    }

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
}

extension BridgeSaveFolderToken: SaveNameToken {
    static let allTokens: [Self] = [
        .artist, .album, .year, .label, .catalogNumber,
    ]

    var label: String {
        switch self {
        case .artist: String(localized: "Artist")
        case .album: String(localized: "Album")
        case .year: String(localized: "Release year")
        case .label: String(localized: "Label")
        case .catalogNumber: String(localized: "Catalog number")
        }
    }

    /// Numbers stay unlocalized, as in real folder names.
    var sampleValue: String {
        switch self {
        case .artist: String(localized: "Artist name")
        case .album: String(localized: "Album title")
        case .year: "2020"
        case .label: String(localized: "Label name")
        case .catalogNumber: "HR-1042"
        }
    }

    var dragId: String {
        switch self {
        case .artist: "artist"
        case .album: "album"
        case .year: "year"
        case .label: "label"
        case .catalogNumber: "catalogNumber"
        }
    }
}

#if DEBUG
    #Preview("Save name fields") {
        @Previewable
        @State
        var fileTokens: [BridgeSaveFilenameToken] = [.trackNumber, .title]
        @Previewable
        @State
        var folderTokens: [BridgeSaveFolderToken] = [.artist, .album]
        Form {
            SaveNameField(
                title: "File name",
                tokens: fileTokens,
                setTokens: { fileTokens = $0 },
                preview: BridgeSaveFilenameToken.previewName(
                    tokens: fileTokens,
                    fallback: .title
                ) + ".flac"
            )
            SaveNameField(
                title: "Folder name",
                tokens: folderTokens,
                setTokens: { folderTokens = $0 },
                preview: BridgeSaveFolderToken.previewName(
                    tokens: folderTokens,
                    fallback: .album
                )
            )
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 420)
    }
#endif
