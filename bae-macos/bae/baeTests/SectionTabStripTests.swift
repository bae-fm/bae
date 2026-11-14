import AppKit
import Testing

@testable import bae

/// The title bar's Library and Import strip is a tab group of tabs for
/// assistive technology, reading the one state its pills draw from.
@MainActor
@Suite("Section tab strip")
struct SectionTabStripTests {
    @Test(
        "the strip is a tab group of tab buttons that press and report selection"
    )
    func stripIsATabGroup() throws {
        var pressed: [MainSection] = []
        let model = SectionTabsModel(
            selection: .library,
            onSelect: { pressed.append($0) }
        )
        let strip = SectionTabStripView(model: model)

        #expect(strip.accessibilityRole() == .tabGroup)
        let tabs = try #require(
            strip.accessibilityChildren() as? [NSAccessibilityElement]
        )
        #expect(tabs.map { $0.accessibilityLabel() } == ["Library", "Import"])
        #expect(tabs.allSatisfy { $0.accessibilityRole() == .radioButton })
        #expect(
            tabs.allSatisfy {
                $0.accessibilitySubrole()?.rawValue == "AXTabButton"
            }
        )
        #expect(tabs.map { $0.isAccessibilitySelected() } == [true, false])
        model.selection = .importing
        #expect(tabs.map { $0.isAccessibilitySelected() } == [false, true])
        #expect(tabs[0].accessibilityPerformPress())
        #expect(pressed == [.library])
    }
}
