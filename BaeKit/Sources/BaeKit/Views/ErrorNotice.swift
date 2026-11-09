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
    private let look: NoticeLook
    private let action: ErrorAction?

    /// A notice showing the error's line; its Details, when the error has a
    /// detail, opens the line as the title over the causes.
    public init(
        error: DisplayError,
        tone: StatusTone = .danger,
        look: NoticeLook = .filled,
        action: ErrorAction? = nil
    ) {
        self.error = error
        self.heading = .line
        self.tone = tone
        self.look = look
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
        look: NoticeLook = .filled,
        action: ErrorAction? = nil
    ) {
        self.error = error
        self.heading = .titled(title, advice: advice)
        self.tone = tone
        self.look = look
        self.action = action
    }

    public var body: some View {
        switch look {
        case .filled, .quiet:
            row
        case .centered, .centeredOnStage:
            centered
        }
    }

    private var row: some View {
        HStack(alignment: .top, spacing: ThemeSpace.related) {
            if tone.symbol != nil {
                StatusGlyph(tone, outline: look == .quiet)
                    .themeIcon(.medium)
            }
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                words
                buttons
                    .padding(.top, ThemeSpace.inline)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(tone, look: look)
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
                detailsButton(details)
                if let action {
                    actionButton(action)
                }
            }
        }
        else if let action {
            actionButton(action)
        }
    }

    /// The centred layout: the glyph and the title in one row, with the
    /// buttons in a row under it, Details first.
    private var centered: some View {
        let onStage = look == .centeredOnStage
        let titleColor = onStage ? Theme.onFillSecondary : Color.secondary
        return VStack(spacing: ThemeSpace.group) {
            HStack(
                alignment: .firstTextBaseline,
                spacing: ThemeSpace.compact
            ) {
                if onStage, let symbol = tone.outlineSymbol {
                    Image(systemName: symbol)
                        .foregroundStyle(Theme.onFillSecondary)
                }
                else {
                    StatusGlyph(tone, outline: true, muted: true)
                }
                Text(verbatim: centeredTitle)
                    .themeText(.heading)
                    .fontWeight(.regular)
                    .foregroundStyle(titleColor)
                    .multilineTextAlignment(.center)
            }
            if case .titled(_, let advice?) = heading {
                Text(verbatim: advice)
                    .themeText(.body)
                    .foregroundStyle(titleColor)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: ThemeSpace.group) {
                if let details {
                    detailsButton(details)
                }
                if let action {
                    actionButton(action)
                }
            }
        }
        .frame(maxWidth: .infinity)
        .notice(tone, look: look)
    }

    /// The title without a closing period.
    private var centeredTitle: String {
        let title =
            switch heading {
            case .line: error.line
            case .titled(let title, _): title
            }
        return title.hasSuffix(".") ? String(title.dropLast()) : title
    }

    /// Details opens an alert, which only AppKit has; the iOS app has no
    /// screens that show a notice.
    @ViewBuilder
    private func detailsButton(_ details: ErrorDetails) -> some View {
        #if canImport(AppKit)
            Button("Details") { ErrorDetailsAlert.show(details) }
                .buttonStyle(.borderless)
                .foregroundStyle(.primary)
        #endif
    }

    private func actionButton(_ action: ErrorAction) -> some View {
        Button(action: action.perform) { action.title }
            .tint(.primary)
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

/// Text colours on a notice's tint.
public enum NoticeText {
    /// Primary text made see-through, so it takes the tint's colour.
    public static let secondary = Color.primary.opacity(0.72)
}
