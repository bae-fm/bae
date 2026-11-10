import SwiftUI

/// An error notice's primary button, such as Retry: at the right of the
/// title's line, Details first when the notice has both.
public struct ErrorAction {
    /// One target of a menu action.
    public struct Choice {
        fileprivate let title: String
        fileprivate let perform: () -> Void

        public init(verbatim title: String, perform: @escaping () -> Void) {
            self.title = title
            self.perform = perform
        }
    }

    fileprivate enum Kind {
        case button(() -> Void)
        case menu([Choice])
    }

    fileprivate let title: Text
    fileprivate let isDisabled: Bool
    fileprivate let kind: Kind

    public init(
        _ title: LocalizedStringKey = "Retry",
        isDisabled: Bool = false,
        perform: @escaping () -> Void
    ) {
        self.title = Text(title)
        self.isDisabled = isDisabled
        self.kind = .button(perform)
    }

    /// A choice of targets, such as each album that holds a release.
    public init(_ title: LocalizedStringKey, choices: [Choice]) {
        self.title = Text(title)
        self.isDisabled = false
        self.kind = .menu(choices)
    }

    /// A label already in the person's language.
    public init(
        verbatim title: String,
        isDisabled: Bool = false,
        perform: @escaping () -> Void
    ) {
        self.title = Text(verbatim: title)
        self.isDisabled = isDisabled
        self.kind = .button(perform)
    }
}

/// A user-facing error as a notice: the tone's glyph, then either the error's
/// line or a title the caller words with what to do under it, and buttons. The
/// notice never shows the error itself: when there is a detail, its Details
/// button opens the causes in an alert, with Copy details. The buttons sit at
/// the right of the title's line, Details first.
public struct Notice: View {
    private enum Heading {
        case line(DisplayError)
        case titled(String, advice: String?, error: DisplayError?)
    }

    private let heading: Heading
    private let tone: StatusTone
    private let look: NoticeLook
    private let action: ErrorAction?
    private let onDismiss: (() -> Void)?

    /// A notice showing the error's line; its Details, when the error has a
    /// detail, opens the line as the title over the causes.
    public init(
        error: DisplayError,
        tone: StatusTone = .danger,
        look: NoticeLook = .filled,
        action: ErrorAction? = nil,
        onDismiss: (() -> Void)? = nil
    ) {
        self.heading = .line(error)
        self.tone = tone
        self.look = look
        self.action = action
        self.onDismiss = onDismiss
    }

    /// A notice titled in the caller's words, with `advice` under it. The
    /// error's line isn't shown, so Details is there whenever there is an
    /// error and leads its causes with the line. With no error there is no
    /// Details: the title and advice are the whole notice.
    public init(
        title: String,
        advice: String?,
        error: DisplayError?,
        tone: StatusTone = .danger,
        look: NoticeLook = .filled,
        action: ErrorAction? = nil,
        onDismiss: (() -> Void)? = nil
    ) {
        self.heading = .titled(title, advice: advice, error: error)
        self.tone = tone
        self.look = look
        self.action = action
        self.onDismiss = onDismiss
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
        Group {
            if tone.symbol != nil {
                RowItem {
                    StatusGlyph(tone, outline: look == .quiet)
                        .themeIcon(.medium)
                } content: {
                    titleLine
                }
            }
            else {
                titleLine
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .notice(tone, look: look)
    }

    /// The words, wrapping, with the buttons at the right on the title's line.
    private var titleLine: some View {
        HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                words
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if details != nil || action != nil || onDismiss != nil {
                buttons
            }
        }
    }

    @ViewBuilder
    private var words: some View {
        switch heading {
        case .line(let error):
            Text(error.line)
                .themeText(.body)
                .foregroundStyle(
                    tone == .neutral
                        ? HierarchicalShapeStyle.secondary
                        : HierarchicalShapeStyle.primary
                )
        case .titled(let title, let advice, _):
            Text(verbatim: title)
                .themeText(advice == nil ? .body : .strong)
            if let advice {
                Text(verbatim: advice)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var buttons: some View {
        HStack(spacing: ThemeSpace.related) {
            if let details {
                detailsButton(details)
            }
            if let action {
                actionButton(action)
            }
            if let onDismiss {
                dismissButton(onDismiss)
            }
        }
        .fixedSize()
    }

    /// Takes the notice away without fixing what it reports.
    private func dismissButton(_ onDismiss: @escaping () -> Void) -> some View {
        Button(action: onDismiss) {
            Image(systemName: "xmark")
                .font(.system(size: 14))
                .foregroundStyle(.tertiary)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(.borderless)
        .help("Dismiss")
        .accessibilityLabel(Text("Dismiss"))
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
            if case .titled(_, let advice?, _) = heading {
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
            case .line(let error): error.line
            case .titled(let title, _, _): title
            }
        return title.hasSuffix(".") ? String(title.dropLast()) : title
    }

    /// Details opens an alert, which only AppKit has; the iOS app has no
    /// screens that show a notice.
    @ViewBuilder
    private func detailsButton(_ details: ErrorDetails) -> some View {
        #if canImport(AppKit)
            colouredButton(
                Button("Details") { ErrorDetailsAlert.show(details) }
                    .buttonStyle(.borderless)
            )
        #endif
    }

    @ViewBuilder
    private func actionButton(_ action: ErrorAction) -> some View {
        switch action.kind {
        case .button(let perform):
            colouredButton(Button(action: perform) { action.title })
                .disabled(action.isDisabled)
        case .menu(let choices):
            colouredButton(
                Menu {
                    ForEach(choices.indices, id: \.self) { index in
                        Button(
                            choices[index].title,
                            action: choices[index].perform
                        )
                    }
                } label: {
                    action.title
                }
                .menuStyle(.button)
                .fixedSize()
            )
            .disabled(action.isDisabled)
        }
    }

    /// A button in the notice's text: on a toned notice's own fill, its accent
    /// there; in a failed view's centred notice, the grey of the words beside
    /// it (the stage's secondary text on a stage); anywhere else the screen's
    /// text.
    @ViewBuilder
    private func colouredButton(_ button: some View) -> some View {
        if look == .filled && tone.hasNoticeFill {
            button.modifier(NoticeAccent())
        }
        else if look == .centered {
            button.foregroundStyle(.secondary).tint(.secondary)
        }
        else if look == .centeredOnStage {
            button
                .foregroundStyle(Theme.onFillSecondary)
                .tint(Theme.onFillSecondary)
        }
        else {
            button.foregroundStyle(.primary).tint(.primary)
        }
    }

    private var details: ErrorDetails? {
        switch heading {
        case .line(let error): ErrorDetails(error: error)
        case .titled(let title, let advice, let error?):
            ErrorDetails(title: title, advice: advice, error: error)
        case .titled(_, _, nil): nil
        }
    }
}
