import AppKit
import Observation
import SwiftUI

/// One tab of a `TabStrip`: what it selects and the name assistive
/// technology reads for it.
struct TabStripTab<Tab: Hashable>: Equatable {
    let tab: Tab
    let name: String
}

/// This model holds a strip's tabs, the selected one and the press action.
/// The drawing reads it to draw, and the tab elements read it to tell
/// assistive technology which tab is selected and where it is.
@MainActor
@Observable
final class TabStripModel<Tab: Hashable> {
    var tabs: [TabStripTab<Tab>]
    var selection: Tab
    /// Each tab's frame in the strip's top-left origin space, which places
    /// its element for assistive technology.
    var frames: [Tab: CGRect] = [:]
    var onSelect: (Tab) -> Void

    init(
        tabs: [TabStripTab<Tab>],
        selection: Tab,
        onSelect: @escaping (Tab) -> Void
    ) {
        self.tabs = tabs
        self.selection = selection
        self.onSelect = onSelect
    }

    /// The name of the coordinate space the drawing reports frames in.
    nonisolated static var space: String { "tabStrip" }
}

extension View {
    /// Reports this view's frame as `tab`'s, so the strip places the tab's
    /// element over it.
    func tabStripFrame<Tab: Hashable>(
        _ tab: Tab,
        in model: TabStripModel<Tab>
    ) -> some View {
        onGeometryChange(for: CGRect.self) { geometry in
            geometry.frame(in: .named(TabStripModel<Tab>.space))
        } action: { frame in
            model.frames[tab] = frame
        }
    }
}

/// Tabs drawn in SwiftUI inside one AppKit view, which is itself the
/// accessibility tab group whose children are the tabs. Each tab is a radio
/// button with the tab-button subrole, the role AppKit's own tab views give
/// their tabs, which SwiftUI cannot give a custom control. The drawing marks
/// each tab's frame with `tabStripFrame`.
struct TabStrip<Tab: Hashable, Drawing: View>: NSViewRepresentable {
    /// The window's environment, which the hosted drawing would otherwise
    /// not get: a hosting view inside a representable starts with a fresh
    /// one.
    @Environment(\.self)
    private var environment
    let tabs: [TabStripTab<Tab>]
    let selection: Tab
    let onSelect: (Tab) -> Void
    /// Whether the strip takes the width its row offers, as tabs that share
    /// a row equally do, rather than its drawing's own width.
    var fillsWidth = false
    @ViewBuilder
    let drawing: (TabStripModel<Tab>) -> Drawing

    func makeNSView(context _: Context) -> TabStripView<Tab> {
        TabStripView(
            model: TabStripModel(
                tabs: tabs,
                selection: selection,
                onSelect: onSelect
            )
        )
    }

    func updateNSView(_ view: TabStripView<Tab>, context _: Context) {
        if view.model.tabs != tabs { view.model.tabs = tabs }
        view.model.selection = selection
        view.model.onSelect = onSelect
        view.show(
            drawing(view.model)
                .coordinateSpace(.named(TabStripModel<Tab>.space))
                .transformEnvironment(\.self) { $0 = environment }
        )
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView view: TabStripView<Tab>,
        context _: Context
    ) -> CGSize? {
        let fitting = view.fittingSize
        guard fillsWidth, let width = proposal.width else { return fitting }
        return CGSize(width: width, height: fitting.height)
    }
}

final class TabStripView<Tab: Hashable>: NSView {
    let model: TabStripModel<Tab>
    private let drawing: NSHostingView<AnyView>
    /// One element per tab, made again when the tabs change.
    private var elements: [TabStripElement<Tab>] = []

    override var isFlipped: Bool { true }

    @MainActor
    init(model: TabStripModel<Tab>) {
        self.model = model
        drawing = NSHostingView(rootView: AnyView(EmptyView()))
        super.init(frame: .zero)
        // The strip's own size and the four edge constraints below size the
        // drawing; the hosting view's minimum and maximum size constraints
        // would conflict with them, and each change to one would make the
        // strip measure itself again.
        drawing.sizingOptions = [.intrinsicContentSize]
        // In the title bar the hosting view would inset the drawing by the
        // bar's safe area, so the tabs would draw, and take clicks, below the
        // frames `tabStripFrame` reports and the elements announce.
        drawing.safeAreaRegions = []
        drawing.translatesAutoresizingMaskIntoConstraints = false
        addSubview(drawing)
        NSLayoutConstraint.activate([
            drawing.leadingAnchor.constraint(equalTo: leadingAnchor),
            drawing.trailingAnchor.constraint(equalTo: trailingAnchor),
            drawing.topAnchor.constraint(equalTo: topAnchor),
            drawing.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    /// Draws `content`, the caller's drawing in the strip's environment.
    func show(_ content: some View) {
        drawing.rootView = AnyView(content)
    }

    override var intrinsicContentSize: NSSize { drawing.fittingSize }

    /// The tab at `index` in the strip, while the strip has one there.
    func tab(at index: Int) -> TabStripTab<Tab>? {
        model.tabs.indices.contains(index) ? model.tabs[index] : nil
    }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? { .tabGroup }

    override func accessibilityChildren() -> [Any]? { tabElements }

    override func accessibilityTabs() -> [Any]? { tabElements }

    override func accessibilityContents() -> [Any]? { tabElements }

    /// The tab elements for the model's tabs as they are now: one per
    /// place in the strip, each reading the tab at its place.
    private var tabElements: [TabStripElement<Tab>] {
        if elements.count != model.tabs.count {
            elements = model.tabs.indices.map {
                TabStripElement(strip: self, index: $0)
            }
        }
        return elements
    }
}

/// One tab, the one at `index` in the strip, reading the strip's model for
/// its name, state and place. Accessibility asks on the main thread.
private final class TabStripElement<Tab: Hashable>: NSAccessibilityElement {
    private weak var strip: TabStripView<Tab>?
    private let index: Int

    init(strip: TabStripView<Tab>, index: Int) {
        self.strip = strip
        self.index = index
        super.init()
    }

    override func accessibilityParent() -> Any? { strip }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .radioButton
    }

    override func accessibilitySubrole() -> NSAccessibility.Subrole? {
        NSAccessibility.Subrole(rawValue: "AXTabButton")
    }

    override func accessibilityLabel() -> String? {
        let strip = strip
        let index = index
        return MainActor.assumeIsolated {
            strip?.tab(at: index)?.name
        }
    }

    override func isAccessibilityEnabled() -> Bool { true }

    override func isAccessibilitySelected() -> Bool {
        let strip = strip
        let index = index
        return MainActor.assumeIsolated {
            guard let strip, let tab = strip.tab(at: index) else {
                return false
            }
            return strip.model.selection == tab.tab
        }
    }

    override func accessibilityFrame() -> NSRect {
        let strip = strip
        let index = index
        return MainActor.assumeIsolated {
            guard let strip, let window = strip.window,
                let tab = strip.tab(at: index),
                let frame = strip.model.frames[tab.tab]
            else { return .zero }
            return window.convertToScreen(strip.convert(frame, to: nil))
        }
    }

    override func accessibilityPerformPress() -> Bool {
        let strip = strip
        let index = index
        return MainActor.assumeIsolated {
            guard let strip, let tab = strip.tab(at: index) else {
                return false
            }
            strip.model.onSelect(tab.tab)
            return true
        }
    }
}
