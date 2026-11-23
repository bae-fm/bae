import BaeKit
import SwiftUI

/// The title bar's Library and Import tabs: pills in a well, with the
/// selected section raised in it.
struct MacSectionTabs: View {
    let selection: MainSection
    let onSelect: (MainSection) -> Void

    static let tabs: [TabStripTab<MainSection>] = [
        TabStripTab(tab: .library, name: String(localized: "Library")),
        TabStripTab(tab: .importing, name: String(localized: "Import")),
    ]

    var body: some View {
        TabStrip(
            tabs: Self.tabs,
            selection: selection,
            onSelect: onSelect
        ) { model in
            SectionPills(model: model)
        }
    }
}

/// The pills' drawing: a well with the selected section raised in it.
private struct SectionPills: View {
    let model: TabStripModel<MainSection>

    var body: some View {
        HStack(spacing: 0) {
            ForEach(model.tabs, id: \.tab) { entry in
                pill(entry.name, section: entry.tab)
            }
        }
        .padding(ThemeSpace.inline)
        .background(
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(Theme.well)
        )
    }

    private func pill(_ title: String, section: MainSection) -> some View {
        let active = model.selection == section
        return Button {
            model.onSelect(section)
        } label: {
            Text(verbatim: title)
                .themeText(.strong)
                .foregroundStyle(
                    active ? Theme.textPrimary : Theme.textSecondary
                )
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
        .tabStripFrame(section, in: model)
    }
}
