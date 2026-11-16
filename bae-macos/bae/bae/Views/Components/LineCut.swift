import SwiftUI

/// Where the view cuts one-line text that does not fit, by what the text is
/// (spec/words.md): a file or folder name keeps its start and its extension or
/// date; a title, a person's name or a sentence keeps its start and loses its
/// end.
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
    /// Limits the view to one line and cuts the text with "…" where `cut`
    /// says.
    func cutLine(_ cut: LineCut) -> some View {
        lineLimit(1).truncationMode(cut.truncation)
    }
}
