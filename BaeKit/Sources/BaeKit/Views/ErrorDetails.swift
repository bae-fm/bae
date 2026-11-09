import Foundation

/// What a notice's Details button opens: a title, an optional message, the
/// causes one a line, and the whole text to copy. A notice never shows the
/// error itself, so what it leaves out is here.
public struct ErrorDetails: Equatable, Sendable {
    public let title: String
    public let message: String?
    /// The causes as shown, bounded.
    public let causes: String
    /// The causes whole.
    public let copyText: String

    /// A notice showing the error's line: the line is the title, and the
    /// causes are the detail. Without a detail there is nothing to open.
    public init?(error: DisplayError) {
        guard let detail = error.detail, let excerpt = error.detailExcerpt
        else {
            return nil
        }
        self.title = error.line
        self.message = nil
        self.causes = excerpt
        self.copyText = detail
    }

    /// A notice titled in its own words, with what to do under it: the line
    /// leads the causes and the copy, since the notice doesn't show it.
    public init(title: String, advice: String?, error: DisplayError) {
        self.title = title
        self.message = advice
        self.causes = [error.line, error.detailExcerpt]
            .compactMap { $0 }.joined(separator: "\n")
        self.copyText = [error.line, error.detail]
            .compactMap { $0 }.joined(separator: "\n")
    }
}
