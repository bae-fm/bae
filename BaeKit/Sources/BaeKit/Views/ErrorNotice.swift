import SwiftUI

/// A button that belongs to an error, such as Retry: it sits inside the
/// error's box, at its right, level with the message.
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

/// A user-facing error as its box: the tone's glyph, the localized line, an
/// optional action at the right, and Details, which opens an inset panel of
/// the causes with Copy details in its corner.
///
/// `bare` leaves out the box and its glyph, for an error already inside a
/// notice that holds it. The line is primary text on the tint; secondary text
/// is primary text made see-through.
public struct ErrorNotice: View {
    private let error: DisplayError
    private let tone: StatusTone
    private let bare: Bool
    private let action: ErrorAction?

    public init(
        error: DisplayError,
        tone: StatusTone = .danger,
        bare: Bool = false,
        action: ErrorAction? = nil
    ) {
        self.error = error
        self.tone = tone
        self.bare = bare
        self.action = action
    }

    @State
    private var detailExpanded = false

    public var body: some View {
        if bare {
            content
        }
        else {
            HStack(alignment: .top, spacing: ThemeSpace.related) {
                if tone.symbol != nil {
                    StatusGlyph(tone)
                        .themeIcon(.medium)
                }
                content
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .notice(tone)
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
                Text(error.line)
                    .themeText(.body)
                    .foregroundStyle(
                        tone == .neutral ? NoticeText.secondary : Color.primary
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let action {
                    Button(action: action.perform) { action.title }
                        .disabled(action.isDisabled)
                }
            }

            if error.detail != nil {
                detailsToggle
                if detailExpanded, let detail = error.detail,
                    let excerpt = error.detailExcerpt
                {
                    ErrorCausesPanel(detail: detail, excerpt: excerpt)
                }
            }
        }
    }

    /// Details starts in line with the message; its chevron hangs in the
    /// margin to the left.
    private var detailsToggle: some View {
        Button {
            detailExpanded.toggle()
        } label: {
            HStack(spacing: 0) {
                Image(systemName: "chevron.right")
                    .themeIcon(.small)
                    .rotationEffect(.degrees(detailExpanded ? 90 : 0))
                    .frame(width: ThemeSpace.hang, alignment: .leading)
                Text("Details")
                    .themeText(.detail)
            }
            .foregroundStyle(NoticeText.secondary)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.leading, -ThemeSpace.hang)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The inset panel of an error's causes, one a line and ending in the fault,
/// with Copy details in its corner.
struct ErrorCausesPanel: View {
    let detail: String
    let excerpt: String
    /// The panel sits on a notice's tint, where text is primary text made
    /// see-through.
    var onTint = true

    @Environment(\.colorScheme)
    private var colorScheme

    var body: some View {
        let text: Color = onTint ? NoticeText.secondary : Color.secondary
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
