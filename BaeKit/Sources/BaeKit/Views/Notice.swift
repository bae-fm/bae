import SwiftUI

/// A notice's button, such as Retry: at the right of the title's line,
/// Details first when the notice has both.
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
    fileprivate let shortcut: KeyboardShortcut?
    fileprivate let kind: Kind

    /// `shortcut` presses the button from anywhere in the window, such as
    /// Escape for a Cancel that leaves the screen.
    public init(
        _ title: LocalizedStringKey = "Retry",
        isDisabled: Bool = false,
        shortcut: KeyboardShortcut? = nil,
        perform: @escaping () -> Void
    ) {
        self.title = Text(title)
        self.isDisabled = isDisabled
        self.shortcut = shortcut
        self.kind = .button(perform)
    }

    /// A choice of targets, such as each album that holds a release.
    public init(_ title: LocalizedStringKey, choices: [Choice]) {
        self.title = Text(title)
        self.isDisabled = false
        self.shortcut = nil
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
        self.shortcut = nil
        self.kind = .button(perform)
    }
}

/// Where a paragraph notice's buttons go.
public enum NoticeButtons {
    /// On the title's line at the right, the words wrapping beside them.
    case inline
    /// On a line of their own under the words, in line with them: for a long
    /// button in a narrow place. The × stays on the title's line.
    case below
}

/// What opens a notice's details.
public enum NoticeDetailsOpening {
    /// A Details button among the notice's buttons.
    case button
    /// A click on the notice's row; it has no Details button. For a list of
    /// problems, where each row is the whole way in. The row is the title;
    /// what to do and the causes are in what it opens.
    case row
}

/// A user-facing error as a notice: the tone's glyph, then either the error's
/// line or a title the caller words with what to do under it, and buttons. The
/// notice never shows the error itself: when there is a detail, its Details
/// button opens the causes in an alert, with Copy details. The buttons sit at
/// the right of the title's line: Details first, then the secondary button,
/// such as Cancel, then the action.
public struct Notice: View {
    private enum Heading {
        case line(DisplayError)
        case titled(String, advice: String?, error: DisplayError?)
    }

    private let heading: Heading
    private let tone: StatusTone
    private let look: NoticeLook
    private let action: ErrorAction?
    private let secondary: ErrorAction?
    private let buttons: NoticeButtons
    private let detailsOpening: NoticeDetailsOpening
    private let onDismiss: (() -> Void)?
    #if canImport(AppKit)
        private var showErrorDetails = ErrorDetailsOpener()
    #endif

    /// A notice showing the error's line; its Details, when the error has a
    /// detail, opens the line as the title over the causes.
    public init(
        error: DisplayError,
        tone: StatusTone = .danger,
        look: NoticeLook = .filled,
        action: ErrorAction? = nil,
        secondary: ErrorAction? = nil,
        buttons: NoticeButtons = .inline,
        detailsOpening: NoticeDetailsOpening = .button,
        onDismiss: (() -> Void)? = nil
    ) {
        self.heading = .line(error)
        self.tone = tone
        self.look = look
        self.action = action
        self.secondary = secondary
        self.buttons = buttons
        self.detailsOpening = detailsOpening
        self.onDismiss = onDismiss
    }

    /// A notice titled in the caller's words, with `advice` under it. The
    /// error's line isn't shown, so Details is there whenever there is an
    /// error and leads its causes with the line. With no error there is no
    /// Details: the title and advice are the whole notice. A notice whose
    /// details open from its row (`detailsOpening: .row`) shows the title
    /// alone; the advice is the message of what the row opens.
    public init(
        title: String,
        advice: String?,
        error: DisplayError?,
        tone: StatusTone = .danger,
        look: NoticeLook = .filled,
        action: ErrorAction? = nil,
        secondary: ErrorAction? = nil,
        buttons: NoticeButtons = .inline,
        detailsOpening: NoticeDetailsOpening = .button,
        onDismiss: (() -> Void)? = nil
    ) {
        self.heading = .titled(title, advice: advice, error: error)
        self.tone = tone
        self.look = look
        self.action = action
        self.secondary = secondary
        self.buttons = buttons
        self.detailsOpening = detailsOpening
        self.onDismiss = onDismiss
    }

    public var body: some View {
        #if canImport(AppKit)
            laidOut.errorDetailsAlert(showErrorDetails.presented)
        #else
            laidOut
        #endif
    }

    @ViewBuilder
    private var laidOut: some View {
        switch look {
        case .filled, .quiet:
            row
        case .centered, .centeredOnStage:
            centered
        }
    }

    private var row: some View {
        rowOpeningDetails(rowContent)
    }

    private var rowContent: some View {
        Group {
            if tone.symbol != nil {
                RowItem {
                    StatusGlyph(tone, outline: true)
                        .font(Self.glyphFont)
                        .accessibilityHidden(true)
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

    /// The glyph is 1.19 times the notice's text, which the boards draw it at.
    private static var glyphFont: Font {
        #if canImport(AppKit)
            .system(size: ThemeText.body.macOSSize * 1.19, weight: .semibold)
        #else
            ThemeIcon.medium.font
        #endif
    }

    /// The words, wrapping, with the buttons at the right on the title's line,
    /// or under the words when the notice puts them `below`.
    private var titleLine: some View {
        HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                words
                if buttons == .below, hasChoiceButtons {
                    choiceButtons.padding(.top, ThemeSpace.inline)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if (buttons == .inline && hasChoiceButtons) || onDismiss != nil {
                HStack(spacing: ThemeSpace.related) {
                    if buttons == .inline {
                        choiceButtons
                    }
                    if let onDismiss {
                        dismissButton(onDismiss)
                    }
                }
                .fixedSize()
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
            let shownAdvice = opensFromRow ? nil : advice
            Text(verbatim: title)
                .themeText(shownAdvice == nil ? .body : .strong)
            if let shownAdvice {
                Text(verbatim: shownAdvice)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
            }
        }
    }

    /// The row opens the details, so its words are the title alone.
    private var opensFromRow: Bool {
        detailsOpening == .row && details != nil
    }

    private var hasChoiceButtons: Bool {
        (details != nil && detailsOpening == .button) || secondary != nil
            || action != nil
    }

    /// Details, when it is a button, then the secondary button, then the
    /// action.
    private var choiceButtons: some View {
        HStack(spacing: ThemeSpace.related) {
            if let details, detailsOpening == .button {
                if buttons == .below {
                    // Under the words Details has the padding of the button
                    // beside it, and pulls out by it so its text lines up with
                    // the words above.
                    detailsButton(details)
                        .padding(.horizontal, ThemeSpace.group)
                        .padding(.leading, -ThemeSpace.group)
                }
                else {
                    detailsButton(details)
                }
            }
            if let secondary {
                actionButton(secondary)
            }
            if let action {
                actionButton(action)
            }
        }
        .fixedSize()
    }

    /// A click anywhere on the row opens the details, for a notice whose
    /// details open from its row; the buttons inside it keep their own clicks.
    @ViewBuilder
    private func rowOpeningDetails(_ content: some View) -> some View {
        #if canImport(AppKit)
            if detailsOpening == .row, let details {
                content
                    .contentShape(Rectangle())
                    .onTapGesture { showErrorDetails(details) }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityAction(named: Text("Details")) {
                        showErrorDetails(details)
                    }
            }
            else {
                content
            }
        #else
            content
        #endif
    }
}

/// The notice's buttons, its centred layout and its details.
extension Notice {
    /// Takes the notice away without fixing what it reports.
    private func dismissButton(_ onDismiss: @escaping () -> Void) -> some View {
        Button(action: onDismiss) {
            Image(systemName: "xmark")
                .font(.system(size: 14))
                .foregroundStyle(.tertiary)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
                // The box reaches past the row's inset and the title's line,
                // so the glyph sits at the inset and the row stays one line.
                .padding(.horizontal, -ThemeSpace.related)
                .padding(.vertical, -(ThemeSpace.compact))
        }
        .buttonStyle(.borderless)
        .help("Dismiss")
        .accessibilityLabel(Text("Dismiss"))
    }

    /// The centred layout: the glyph and the title in one row, with the
    /// buttons in a row under it: Details, the secondary button, the action.
    private var centered: some View {
        let onStage = look == .centeredOnStage
        let secondaryColor = onStage ? Theme.onFillSecondary : Color.secondary
        // Under advice the title is the line to read; a title alone stays
        // grey, like the glyph and buttons beside it.
        let hasAdvice: Bool
        if case .titled(_, _?, _) = heading {
            hasAdvice = true
        }
        else {
            hasAdvice = false
        }
        let titleColor =
            onStage
            ? Theme.onFillSecondary
            : hasAdvice ? Theme.textPrimary : secondaryColor
        return VStack(spacing: ThemeSpace.group) {
            HStack(
                alignment: .firstTextBaseline,
                spacing: ThemeSpace.compact
            ) {
                if onStage, let symbol = tone.outlineSymbol {
                    Image(systemName: symbol)
                        .foregroundStyle(Theme.onFillSecondary)
                        .accessibilityHidden(true)
                }
                else {
                    StatusGlyph(tone, outline: true, muted: true)
                        .accessibilityHidden(true)
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
                    .foregroundStyle(secondaryColor)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: ThemeSpace.group) {
                if let details {
                    detailsButton(details)
                }
                if let secondary {
                    actionButton(secondary)
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
                Button("Details") { showErrorDetails(details) }
                    .buttonStyle(.borderless),
                borderless: true
            )
        #endif
    }

    @ViewBuilder
    private func actionButton(_ action: ErrorAction) -> some View {
        switch action.kind {
        case .button(let perform):
            colouredButton(
                Button(action: perform) { action.title },
                borderless: false
            )
            .disabled(action.isDisabled)
            .keyboardShortcut(action.shortcut)
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
                .fixedSize(),
                borderless: false
            )
            .disabled(action.isDisabled)
        }
    }

    /// A button in the notice: a borderless one (Details) in the accent,
    /// apart from a centred notice's grey title; a bordered one (Retry) in
    /// the plain button look, white on the control grey, with no tint. In a
    /// failed view's centred notice Retry takes the grey of the words beside
    /// it; on a stage both take the stage's secondary text.
    @ViewBuilder
    private func colouredButton(_ button: some View, borderless: Bool)
        -> some View
    {
        if look == .centered, !borderless {
            button.foregroundStyle(.secondary).tint(.secondary)
        }
        else if look == .centeredOnStage {
            button
                .foregroundStyle(Theme.onFillSecondary)
                .tint(Theme.onFillSecondary)
        }
        else if borderless {
            button.foregroundStyle(Theme.accent).tint(Theme.accent)
        }
        else {
            button
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

extension DisplayError {
    /// Retry for this failure, running `perform`: nil when core says making
    /// the same call again can't get past it, so the notice offers none.
    public func retryAction(
        isDisabled: Bool = false,
        perform: @escaping () -> Void
    ) -> ErrorAction? {
        offersRetry
            ? ErrorAction(isDisabled: isDisabled, perform: perform) : nil
    }
}
