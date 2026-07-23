import BaeKit

/// One selectable cover in the cover picker: a `BridgeCoverChoice` (its
/// selection identity plus preview/thumbnail sources) and a display label.
struct CoverItem: Identifiable, Equatable {
    static func == (lhs: CoverItem, rhs: CoverItem) -> Bool {
        lhs.id == rhs.id
    }

    var id: BridgeCoverSelection { selection }
    var selection: BridgeCoverSelection { coverChoice.selection }
    var previewSource: ImageLoader.Source {
        ImageLoader.Source(bridge: coverChoice.previewSource)
    }

    var thumbnailSource: ImageLoader.Source {
        ImageLoader.Source(bridge: coverChoice.thumbnailSource)
    }

    let coverChoice: BridgeCoverChoice
    let label: String

    init(
        coverChoice: BridgeCoverChoice,
        label: String
    ) {
        self.coverChoice = coverChoice
        self.label = label
    }
}
