import BaeKit
import SwiftUI

/// The Found / Imported / Skipped tabs above the candidate list, each named
/// with its count from the list's count, which a filter does not change.
/// The bar reads the counts from `listSlot` itself, so a count that changes
/// redraws the bar alone.
struct TriageTabBar: View {
    @Binding
    var activeTab: BridgeTriageTab
    let listSlot: ImportListSlot

    static let order: [BridgeTriageTab] = [.pending, .done, .skipped]

    var body: some View {
        // `tabCounts` is nil until the list's count arrives, and each chip
        // stays empty until then.
        let counts = listSlot.tabCounts
        TabStrip(
            tabs: Self.order.map { tab in
                TabStripTab(
                    tab: tab,
                    name: Self.name(of: tab, count: counts.map(tab.count(in:)))
                )
            },
            selection: activeTab,
            onSelect: select,
            fillsWidth: true
        ) { model in
            HStack(spacing: ThemeSpace.inline) {
                ForEach(Self.order, id: \.self) { tab in
                    segment(
                        tab,
                        count: counts.map(tab.count(in:)),
                        model: model
                    )
                }
            }
        }
    }

    private func select(_ tab: BridgeTriageTab) {
        activeTab = tab
    }

    /// What assistive technology calls a tab: its name, then its count once
    /// the count has arrived.
    static func name(of tab: BridgeTriageTab, count: UInt32?) -> String {
        let label = MacImportFolderLine.label(for: tab)
        guard let count else { return label }
        return String(localized: "\(label), \(count.formatted())")
    }

    private func segment(
        _ tab: BridgeTriageTab,
        count: UInt32?,
        model: TabStripModel<BridgeTriageTab>
    ) -> some View {
        let isActive = model.selection == tab
        return Button {
            model.onSelect(tab)
        } label: {
            HStack(spacing: ThemeSpace.inline) {
                Text(verbatim: MacImportFolderLine.label(for: tab))
                    .themeText(.strong)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                let tone: StatusTone = isActive ? .accent : .neutral
                Text(verbatim: count.map { $0.formatted() } ?? "")
                    .themeText(.chip)
                    .monospacedDigit()
                    .foregroundStyle(tone.color)
                    .lineLimit(1)
                    .padding(.horizontal, ThemeSpace.compact)
                    .padding(.vertical, ThemeSpace.line)
                    .background(
                        tone.fill,
                        in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    )
                    // A squeezed badge must overflow, never stack its digits.
                    .fixedSize()
            }
            .foregroundStyle(isActive ? Theme.accent : Color.secondary)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, ThemeSpace.related)
            .padding(.vertical, ThemeSpace.compact)
            .background(
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(isActive ? Theme.accentSoft : Color.clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.accentPlain)
        .tabStripFrame(tab, in: model)
    }
}

extension BridgeTriageTab {
    /// The tab's count among `counts`.
    func count(in counts: BridgeTriageTabCounts) -> UInt32 {
        switch self {
        case .pending: counts.pending
        case .done: counts.done
        case .skipped: counts.skipped
        }
    }
}
