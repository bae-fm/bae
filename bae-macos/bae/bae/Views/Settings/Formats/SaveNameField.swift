import BaeKit
import SwiftUI

/// A value a save name pattern fills in, as the pane's chips offer it.
protocol SaveNameValue: Hashable {
    /// Every value, in the order the "Add:" row offers them.
    static var allValues: [Self] { get }
    var label: String { get }
    /// This value in the settings preview.
    var sampleValue: String { get }
    /// The string a chip drag carries; it never leaves the drag session.
    var dragId: String { get }
}

/// What a piece of a pattern is.
enum SaveNamePieceContent<Value> {
    case value(Value)
    case text(String)
}

/// A piece of a save name pattern: a value, shown as a chip, or the text the
/// person typed.
protocol SaveNamePiece: Hashable {
    associatedtype Value: SaveNameValue
    init(value: Value)
    init(text: String)
    var content: SaveNamePieceContent<Value> { get }
}

extension SaveNamePiece {
    /// The settings preview of a pattern's name: each value as its sample,
    /// the typed text as typed.
    static func previewName(pieces: [Self]) -> String {
        pieces.map { piece in
            switch piece.content {
            case .value(let value): value.sampleValue
            case .text(let text): text
            }
        }
        .joined()
    }
}

extension BridgeNamePatternError {
    /// Why the pattern can't make a name, in the field's words.
    var message: String {
        switch self {
        case .empty:
            String(localized: "A name cannot be empty.")
        case .startsWithDot:
            String(localized: "A name cannot start with a dot.")
        case .forbiddenCharacter(let character):
            String(localized: "A name cannot contain “\(character)”.")
        }
    }
}

/// A name pattern Save as… writes with: its title above, the pattern as a
/// field to type in with its values as chips, a preview of a name it makes
/// (the reason it can't make one, while it can't), and an "Add:" row of the
/// values it doesn't hold.
struct SaveNameField<Piece: SaveNamePiece>: View {
    let title: LocalizedStringKey
    let pieces: [Piece]
    /// Why core says the pattern can't make a name.
    let error: BridgeNamePatternError?
    /// What follows the preview's name: the extension of the file it names.
    var previewSuffix = ""
    /// Store the pattern, in the order the field changes it.
    let setPieces: @MainActor ([Piece]) async throws -> Void
    let fail: @MainActor (Error) -> Void

    @State
    private var handle = SaveNameFieldHandle<Piece>()
    @State
    private var focused = false

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(title)
                .themeText(.body)
            SaveNameTokenField(
                pieces: pieces,
                handle: handle,
                focused: $focused,
                setPieces: setPieces,
                fail: fail
            )
            .accessibilityLabel(Text(title))
            .modifier(
                FieldChrome(
                    focused: focused,
                    style: .boxedMultiline,
                    invalid: error != nil
                )
            )
            // The row aligns by its title's and note's text, not the field's.
            // An NSTokenField asked for its text baseline while it is typed
            // in takes in the editor's text and marks its size changed, so
            // SwiftUI measured it and asked again, without end.
            .alignmentGuide(.firstTextBaseline) { $0[.bottom] }
            .alignmentGuide(.lastTextBaseline) { $0[.bottom] }
            if let error {
                Text(error.message)
                    .themeText(.detail)
                    .foregroundStyle(Theme.danger)
            }
            else {
                Text(
                    "Preview: \(Text(Piece.previewName(pieces: pieces) + previewSuffix).monospaced())"
                )
                .fieldNote()
            }
            if !availableValues.isEmpty {
                addRow
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var addRow: some View {
        FlowLayout(spacing: ThemeSpace.compact) {
            Text("Add:")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            ForEach(availableValues, id: \.self) { value in
                Button(value.label) {
                    handle.append(value)
                }
                .buttonStyle(.accentPlain)
                .themeText(.detail)
                .padding(.horizontal, ThemeSpace.compact)
                .padding(.vertical, ThemeSpace.hairline)
                .overlay(
                    RoundedRectangle(cornerRadius: ThemeRadius.chip)
                        .stroke(
                            Theme.hairlineStrong,
                            style: StrokeStyle(lineWidth: 1, dash: [3, 2])
                        )
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var availableValues: [Piece.Value] {
        let held = Set(
            pieces.compactMap { piece -> Piece.Value? in
                guard case .value(let value) = piece.content else { return nil }
                return value
            }
        )
        return Piece.Value.allValues.filter { !held.contains($0) }
    }
}

extension BridgeSaveFilenamePiece: SaveNamePiece {
    init(value: BridgeSaveFilenameValue) {
        self = .value(value: value)
    }

    init(text: String) {
        self = .text(text: text)
    }

    var content: SaveNamePieceContent<BridgeSaveFilenameValue> {
        switch self {
        case .value(let value): .value(value)
        case .text(let text): .text(text)
        }
    }
}

extension BridgeSaveFolderPiece: SaveNamePiece {
    init(value: BridgeSaveFolderValue) {
        self = .value(value: value)
    }

    init(text: String) {
        self = .text(text: text)
    }

    var content: SaveNamePieceContent<BridgeSaveFolderValue> {
        switch self {
        case .value(let value): .value(value)
        case .text(let text): .text(text)
        }
    }
}

extension BridgeSaveFilenameValue: SaveNameValue {
    static let allValues: [Self] = [
        .trackNumber, .title, .artist, .album, .albumYear, .releaseYear,
        .discNumber, .trackTotal,
    ]

    var label: String {
        switch self {
        case .title: String(localized: "Title")
        case .artist: String(localized: "Artist")
        case .album: String(localized: "Album")
        case .albumYear: String(localized: "Album year")
        case .releaseYear: String(localized: "Release year")
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
        case .albumYear: "1992"
        case .releaseYear: "2020"
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
        case .albumYear: "albumYear"
        case .releaseYear: "releaseYear"
        case .trackNumber: "trackNumber"
        case .discNumber: "discNumber"
        case .trackTotal: "trackTotal"
        }
    }
}

extension BridgeSaveFolderValue: SaveNameValue {
    static let allValues: [Self] = [
        .artist, .album, .albumYear, .releaseYear, .label, .catalogNumber,
    ]

    var label: String {
        switch self {
        case .artist: String(localized: "Artist")
        case .album: String(localized: "Album")
        case .albumYear: String(localized: "Album year")
        case .releaseYear: String(localized: "Release year")
        case .label: String(localized: "Label")
        case .catalogNumber: String(localized: "Catalog number")
        }
    }

    /// Numbers stay unlocalized, as in real folder names.
    var sampleValue: String {
        switch self {
        case .artist: String(localized: "Artist name")
        case .album: String(localized: "Album title")
        case .albumYear: "1992"
        case .releaseYear: "2020"
        case .label: String(localized: "Label name")
        case .catalogNumber: "HR-1042"
        }
    }

    var dragId: String {
        switch self {
        case .artist: "artist"
        case .album: "album"
        case .albumYear: "albumYear"
        case .releaseYear: "releaseYear"
        case .label: "label"
        case .catalogNumber: "catalogNumber"
        }
    }
}
