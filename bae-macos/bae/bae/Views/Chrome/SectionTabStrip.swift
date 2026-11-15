import AppKit
import BaeKit
import Observation
import SwiftUI

/// What the Library/Import strip shows and does, held once: the pills read it
/// to draw and the strip's tabs read it to report.
@MainActor
@Observable
final class SectionTabsModel {
    var selection: MainSection
    /// Each pill's frame in the strip's top-left origin space, which places
    /// its tab for assistive technology.
    var frames: [MainSection: CGRect] = [:]
    var onSelect: (MainSection) -> Void

    static let sections: [(section: MainSection, title: String)] = [
        (.library, String(localized: "Library")),
        (.importing, String(localized: "Import")),
    ]

    init(
        selection: MainSection,
        onSelect: @escaping (MainSection) -> Void
    ) {
        self.selection = selection
        self.onSelect = onSelect
    }
}

/// The Library/Import tabs as one AppKit view: it hosts the pills' drawing
/// and is itself the accessibility tab group, whose children are the tabs.
/// Each tab is a radio button with the tab-button subrole, the role AppKit's
/// own tab views give their tabs, which SwiftUI cannot give a custom control.
struct SectionTabStrip: NSViewRepresentable {
    /// The window's environment, which the hosted pills would otherwise not
    /// get: a hosting view inside a representable starts with a fresh one.
    @Environment(\.self)
    private var environment
    let selection: MainSection
    let onSelect: (MainSection) -> Void

    func makeNSView(context _: Context) -> SectionTabStripView {
        SectionTabStripView(
            model: SectionTabsModel(selection: selection, onSelect: onSelect)
        )
    }

    func updateNSView(_ view: SectionTabStripView, context _: Context) {
        view.model.selection = selection
        view.model.onSelect = onSelect
        view.show(environment: environment)
    }

    func sizeThatFits(
        _: ProposedViewSize,
        nsView view: SectionTabStripView,
        context _: Context
    ) -> CGSize? {
        view.fittingSize
    }
}

final class SectionTabStripView: NSView {
    let model: SectionTabsModel
    private let pills: NSHostingView<AnyView>
    private var tabs: [SectionTabElement] = []

    override var isFlipped: Bool { true }

    @MainActor
    init(model: SectionTabsModel) {
        self.model = model
        pills = NSHostingView(rootView: AnyView(SectionPills(model: model)))
        super.init(frame: .zero)
        pills.translatesAutoresizingMaskIntoConstraints = false
        addSubview(pills)
        NSLayoutConstraint.activate([
            pills.leadingAnchor.constraint(equalTo: leadingAnchor),
            pills.trailingAnchor.constraint(equalTo: trailingAnchor),
            pills.topAnchor.constraint(equalTo: topAnchor),
            pills.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        tabs = SectionTabsModel.sections.map {
            SectionTabElement(strip: self, section: $0.section, title: $0.title)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    /// Draws the pills in `environment`, the one the strip sits in.
    func show(environment: EnvironmentValues) {
        pills.rootView = AnyView(
            SectionPills(model: model)
                .transformEnvironment(\.self) { $0 = environment }
        )
    }

    override var intrinsicContentSize: NSSize { pills.fittingSize }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? { .tabGroup }

    override func accessibilityChildren() -> [Any]? { tabs }

    override func accessibilityTabs() -> [Any]? { tabs }

    override func accessibilityContents() -> [Any]? { tabs }
}

/// One tab, reading the strip's model for its state and place.
private final class SectionTabElement: NSAccessibilityElement {
    private weak var strip: SectionTabStripView?
    private let section: MainSection
    private let title: String

    init(strip: SectionTabStripView, section: MainSection, title: String) {
        self.strip = strip
        self.section = section
        self.title = title
        super.init()
    }

    override func accessibilityParent() -> Any? { strip }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .radioButton
    }

    override func accessibilitySubrole() -> NSAccessibility.Subrole? {
        NSAccessibility.Subrole(rawValue: "AXTabButton")
    }

    override func accessibilityLabel() -> String? { title }

    override func isAccessibilityEnabled() -> Bool { true }

    override func isAccessibilitySelected() -> Bool {
        nonisolated(unsafe) let strip = strip
        let section = section
        return MainActor.assumeIsolated {
            strip?.model.selection == section
        }
    }

    // Accessibility asks on the main thread.
    override func accessibilityFrame() -> NSRect {
        nonisolated(unsafe) let strip = strip
        let section = section
        return MainActor.assumeIsolated {
            guard let strip, let window = strip.window,
                let frame = strip.model.frames[section]
            else { return .zero }
            return window.convertToScreen(strip.convert(frame, to: nil))
        }
    }

    override func accessibilityPerformPress() -> Bool {
        nonisolated(unsafe) let strip = strip
        let section = section
        return MainActor.assumeIsolated {
            guard let strip else { return false }
            strip.model.onSelect(section)
            return true
        }
    }
}

/// The pills' drawing: a well with the selected section raised in it.
private struct SectionPills: View {
    let model: SectionTabsModel
    private static let space = "sectionTabStrip"

    var body: some View {
        HStack(spacing: 0) {
            ForEach(SectionTabsModel.sections, id: \.section) { entry in
                pill(entry.title, section: entry.section)
            }
        }
        .padding(ThemeSpace.inline)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(Theme.well)
        )
        .coordinateSpace(.named(Self.space))
    }

    private func pill(_ title: String, section: MainSection) -> some View {
        let active = model.selection == section
        return Button {
            model.onSelect(section)
        } label: {
            Text(verbatim: title)
                .themeText(.strong)
                .foregroundStyle(active ? Theme.textPrimary : Color.secondary)
                .padding(.horizontal, ThemeSpace.edge)
                .padding(.vertical, ThemeSpace.compact)
                .background(
                    // Concentric with the well around it.
                    RoundedRectangle(
                        cornerRadius: ThemeRadius.control - ThemeSpace.inline
                    )
                    .fill(Theme.tile)
                    .opacity(active ? 1 : 0)
                    .shadow(
                        color: active ? Theme.shadow : Color.clear,
                        radius: 1.5,
                        y: 1
                    )
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .animation(.easeInOut(duration: 0.15), value: active)
        .onGeometryChange(for: CGRect.self) { geometry in
            geometry.frame(in: .named(Self.space))
        } action: { frame in
            model.frames[section] = frame
        }
    }
}
