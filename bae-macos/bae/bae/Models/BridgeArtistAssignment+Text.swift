import BaeKit
import Foundation

extension BridgeArtistAssignment {
    static func named(_ name: String) -> Self {
        .new(
            seed: BridgeNewArtistSeed(
                name: name,
                sortName: nil,
                musicbrainzArtistId: nil,
                discogsArtistId: nil
            )
        )
    }

    var editorText: String {
        switch self {
        case .existing(let artistId): artistId
        case .new(let seed): seed.name
        }
    }
}

extension Array where Element == BridgeArtistAssignment {
    var editorText: String {
        map(\.editorText).joined(separator: ", ")
    }

    func replacingEditorText(_ text: String) -> Self {
        guard text != editorText else { return self }
        return text.split(separator: ",", omittingEmptySubsequences: false)
            .map {
                BridgeArtistAssignment.named(
                    String($0).trimmingCharacters(in: .whitespaces)
                )
            }
    }
}

extension BridgeTrackArtistAssignments {
    var editorText: String {
        switch self {
        case .albumArtists: ""
        case .explicit(let assignments): assignments.editorText
        }
    }

    func replacingEditorText(_ text: String) -> Self {
        guard text != editorText else { return self }
        if text.isEmpty { return .albumArtists }
        return .explicit(
            assignments: [BridgeArtistAssignment]().replacingEditorText(text)
        )
    }
}
