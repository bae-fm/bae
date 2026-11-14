import SwiftUI

/// Where a one-line text is cut when it does not fit, by what the text is
/// (spec/words.md): a file or folder name keeps its start and its extension or
/// date; a title, a person's name or a sentence reads from its start.
enum LineCut {
    case fileName
    case prose

    var truncation: Text.TruncationMode {
        switch self {
        case .fileName: .middle
        case .prose: .tail
        }
    }
}

extension View {
    /// One line, cut with "…" where `cut` says.
    func cutLine(_ cut: LineCut) -> some View {
        lineLimit(1).truncationMode(cut.truncation)
    }
}
