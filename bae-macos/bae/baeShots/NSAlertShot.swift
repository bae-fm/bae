import AppKit
import SwiftUI

/// An alert as AppKit lays it out: its window's content drawn into an image,
/// since a modal alert can't be captured offscreen. The alert's own views,
/// which hold its icon, title, message and buttons, are what the image shows,
/// so they are listed in the shot's accessibility tree over the image, boxed
/// where the image draws them.
struct NSAlertShot: View {
    let alert: () -> NSAlert

    @Environment(\.colorScheme)
    private var colorScheme

    var body: some View {
        let alert = alert()
        let window = alert.window
        window.appearance = NSAppearance(
            named: colorScheme == .dark ? .darkAqua : .aqua
        )
        alert.layout()
        guard let view = window.contentView,
            let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else {
            preconditionFailure("An alert's window has content to draw")
        }
        let bounds = view.bounds
        view.cacheDisplay(in: bounds, to: rep)
        let image = NSImage(size: bounds.size)
        image.addRepresentation(rep)
        return Image(nsImage: image)
            .accessibilityHidden(true)
            .overlay { AlertElements(content: view) }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Lists `content`'s accessibility tree over a view the size of the image
/// drawn from it.
private struct AlertElements: NSViewRepresentable {
    let content: NSView

    func makeNSView(context: Context) -> AlertElementsView {
        AlertElementsView(content: content)
    }

    func updateNSView(_ view: AlertElementsView, context: Context) {}
}

/// A group, laid out as `content` is, whose children are `content`'s
/// elements.
private final class AlertElementsView: NSView {
    let content: NSView

    init(content: NSView) {
        self.content = content
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        preconditionFailure("Made in code")
    }

    override var isFlipped: Bool { true }

    /// Nothing here draws or takes a click: the image under it does.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? { .group }

    /// The alert's elements are its cells and views, which AppKit lists
    /// under the content view. The accessibility API holds elements weakly,
    /// so they are kept here for as long as the view lives.
    private lazy var elements: [AlertElement] =
        (content.accessibilityChildren() ?? [])
        .compactMap {
            ($0 as? any NSAccessibilityProtocol)
                .map { AlertElement(source: $0, parent: self, view: self) }
        }

    override func accessibilityChildren() -> [Any]? { elements }

    /// The box of `owner`, a view below the alert's content, on the screen
    /// of the window this view is in. The image is the content's pixels,
    /// drawn point for point.
    func screenFrame(of owner: NSView) -> NSRect {
        guard let window else {
            preconditionFailure("The elements are read from a shown window")
        }
        let inContent = content.convert(owner.bounds, from: owner)
        let top =
            content.isFlipped
            ? inContent.minY : content.bounds.height - inContent.maxY
        let local = NSRect(
            x: inContent.minX,
            y: top,
            width: inContent.width,
            height: inContent.height
        )
        return window.convertToScreen(convert(local, to: nil))
    }
}

/// One of the alert's elements, saying what its source says and boxed on
/// the shot's window.
private final class AlertElement: NSAccessibilityElement {
    private let source: any NSAccessibilityProtocol
    private unowned let view: AlertElementsView

    init(
        source: any NSAccessibilityProtocol,
        parent: Any,
        view: AlertElementsView
    ) {
        self.source = source
        self.view = view
        super.init()
        setAccessibilityParent(parent)
    }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? {
        source.accessibilityRole()
    }

    override func accessibilitySubrole() -> NSAccessibility.Subrole? {
        source.accessibilitySubrole()
    }

    override func accessibilityLabel() -> String? {
        source.accessibilityLabel()
    }

    override func accessibilityTitle() -> String? {
        source.accessibilityTitle()
    }

    override func accessibilityValue() -> Any? {
        source.accessibilityValue()
    }

    override func isAccessibilityEnabled() -> Bool {
        source.isAccessibilityEnabled()
    }

    override func accessibilityAttributedString(
        for range: NSRange
    ) -> NSAttributedString? {
        source.accessibilityAttributedString(for: range)
    }

    /// The view the source draws in: itself, or for a cell (which keeps no
    /// box of its own) its control.
    private var owner: NSView? {
        (source as? NSCell)?.controlView ?? source as? NSView
    }

    override func accessibilityFrame() -> NSRect {
        guard let owner else { return .zero }
        return view.screenFrame(of: owner)
    }

    private lazy var elements: [AlertElement] =
        (source.accessibilityChildren() ?? [])
        .compactMap {
            ($0 as? any NSAccessibilityProtocol)
                .map { AlertElement(source: $0, parent: self, view: view) }
        }

    override func accessibilityChildren() -> [Any]? { elements }
}
