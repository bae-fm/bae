import SwiftUI

/// An error notice's primary button, such as Retry: on its own line under the
/// words, in line with them; at the right beside Details when the notice has
/// both.
public struct ErrorAction {
    fileprivate let title: Text
    fileprivate let isDisabled: Bool
    fileprivate let perform: () -> Void

    public init(
        _ title: LocalizedStringKey = "Retry",
        isDisabled: Bool = false,
        perform: @escaping () -> Void
    ) {
        self.title = Text(title)
        self.isDisabled = isDisabled
        self.perform = perform
    }

    /// A label already in the person's language.
    public init(
        verbatim title: String,
        isDisabled: Bool = false,
        perform: @escaping () -> Void
    ) {
        self.title = Text(verbatim: title)
        self.isDisabled = isDisabled
        self.perform = perform
    }
}

/// A user-facing error as a notice: the tone's glyph, then either the error's
/// line or a title the caller words with what to do under it, and buttons. The
/// notice never shows the error itself: when there is a detail, its Details
/// button opens the causes in an alert, with Copy details. With Details and an
/// action the two sit at the right, Details first; with one button it sits on
/// its own line, in line with the words.
public struct ErrorNotice: View {
    private enum Heading {
        case line
        case titled(String, advice: String?)
    }

    private let error: DisplayError
    private let heading: Heading
    private let tone: StatusTone
    private let action: ErrorAction?

    /// A notice showing the error's line; its Details, when the error has a
    /// detail, opens the line as the title over the causes.
    public init(
        error: DisplayError,
        tone: StatusTone = .danger,
        action: ErrorAction? = nil
    ) {
        self.error = error
        self.heading = .line
        self.tone = tone
        self.action = action
    }

    /// A notice titled in the caller's words, with `advice` under it. The
    /// error's line isn't shown, so Details is always there and leads its
    /// causes with the line.
    public init(
        title: String,
        advice: String?,
        error: DisplayError,
        tone: StatusTone = .danger,
        action: ErrorAction? = nil
    ) {
        self.error = error
        self.heading = .titled(title, advice: advice)
        self.tone = tone
        self.action = action
    }

    public var body: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            if tone.symbol != nil {
                StatusGlyph(tone)
                    .themeIcon(.medium)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                words
                buttons
                    .padding(.top, ThemeSpace.inline)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(tone)
    }

    @ViewBuilder
    private var words: some View {
        switch heading {
        case .line:
            Text(error.line)
                .themeText(.body)
                .foregroundStyle(
                    tone == .neutral ? NoticeText.secondary : Color.primary
                )
        case .titled(let title, let advice):
            Text(verbatim: title)
                .themeText(advice == nil ? .body : .strong)
            if let advice {
                Text(verbatim: advice)
                    .themeText(.body)
                    .foregroundStyle(NoticeText.secondary)
            }
        }
    }

    @ViewBuilder
    private var buttons: some View {
        if let details {
            HStack {
                Spacer(minLength: 0)
                Button("Details") { ErrorDetailsAlert.show(details) }
                    .buttonStyle(.borderless)
                if let action {
                    actionButton(action)
                }
            }
        }
        else if let action {
            actionButton(action)
        }
    }

    private func actionButton(_ action: ErrorAction) -> some View {
        Button(action: action.perform) { action.title }
            .disabled(action.isDisabled)
    }

    private var details: ErrorDetails? {
        switch heading {
        case .line: ErrorDetails(error: error)
        case .titled(let title, let advice):
            ErrorDetails(title: title, advice: advice, error: error)
        }
    }
}

/// The inset panel of an error's causes, one a line and ending in the fault,
/// with Copy details in its corner.
struct ErrorCausesPanel: View {
    let detail: String
    let excerpt: String
    @Environment(\.colorScheme)
    private var colorScheme

    var body: some View {
        let text = Color.secondary
        Text(excerpt)
            .themeText(.mono)
            .foregroundStyle(text)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, ThemeSpace.related)
            .padding(.leading, ThemeSpace.related + ThemeSpace.line)
            .padding(.trailing, 30)
            .background(
                Color.black.opacity(colorScheme == .dark ? 0.28 : 0.07),
                in: RoundedRectangle(cornerRadius: ThemeRadius.control)
            )
            .overlay(alignment: .topTrailing) {
                Button {
                    Pasteboard.copy(detail)
                } label: {
                    Label("Copy details", systemImage: "doc.on.doc")
                        .labelStyle(.iconOnly)
                        .themeIcon(.small)
                        .foregroundStyle(text)
                }
                .buttonStyle(.borderless)
                .help("Copy details")
                .padding(ThemeSpace.related)
            }
    }
}

/// Text colours on a notice's tint.
public enum NoticeText {
    /// Primary text made see-through, so it takes the tint's colour.
    public static let secondary = Color.primary.opacity(0.72)
}
