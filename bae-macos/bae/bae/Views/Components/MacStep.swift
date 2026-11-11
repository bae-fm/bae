import AppKit
import BaeKit
import SwiftUI

/// One step of creating, joining or adding a device, centred in the welcome
/// window: a headline, an optional line under it in secondary text, then the
/// step's own content (a field, a code) and its buttons in `MacStepActions`,
/// a section apart. The welcome window has no title bar text, so the headline
/// is the only place that says where you are.
struct MacStep<Content: View>: View {
    let title: String
    let text: String?
    private let content: Content

    init(
        _ title: String,
        text: String? = nil,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.text = text
        self.content = content()
    }

    /// The widest the headline and its line grow.
    private static var headWidth: CGFloat { 440 }

    var body: some View {
        VStack(spacing: ThemeSpace.section) {
            VStack(spacing: ThemeSpace.compact) {
                BalancedText(
                    text: title,
                    font: .systemFont(ofSize: 20, weight: .semibold),
                    color: .labelColor,
                    lineHeight: 26,
                    maxWidth: Self.headWidth
                )
                if let text {
                    BalancedText(
                        text: text,
                        font: .systemFont(ofSize: 13),
                        color: .secondaryLabelColor,
                        lineHeight: 18,
                        maxWidth: Self.headWidth
                    )
                }
            }
            content
        }
        .padding(.top, 56)
        .padding([.horizontal, .bottom], ThemeSpace.page)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Centred text that wraps so its last line never holds one word alone, in
/// any language: an AppKit text view, since SwiftUI's `Text` has no line
/// break strategy to set.
private struct BalancedText: NSViewRepresentable {
    let text: String
    let font: NSFont
    let color: NSColor
    let lineHeight: CGFloat
    let maxWidth: CGFloat

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: "")
        field.isSelectable = false
        field.setContentCompressionResistancePriority(
            .defaultLow,
            for: .horizontal
        )
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        style.lineBreakStrategy = .pushOut
        style.minimumLineHeight = lineHeight
        style.maximumLineHeight = lineHeight
        field.attributedStringValue = NSAttributedString(
            string: text,
            attributes: [
                .font: font,
                .foregroundColor: color,
                .paragraphStyle: style,
            ]
        )
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView field: NSTextField,
        context: Context
    ) -> CGSize? {
        let width = min(proposal.width ?? maxWidth, maxWidth)
        field.preferredMaxLayoutWidth = width
        let height = field.cell?
            .cellSize(
                forBounds: NSRect(
                    x: 0,
                    y: 0,
                    width: width,
                    height: .greatestFiniteMagnitude
                )
            )
            .height
        return CGSize(width: width, height: ceil(height ?? lineHeight))
    }
}

/// A step's buttons, side by side and centred under what they act on, the
/// default on the right: list it last.
struct MacStepActions<Content: View>: View {
    private let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            content
        }
    }
}
