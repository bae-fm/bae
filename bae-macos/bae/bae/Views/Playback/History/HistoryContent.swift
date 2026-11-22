import BaeKit
import SwiftUI

/// The side pane's History tab: the plays this device holds, newest first,
/// under the day they were played, everyone's or one person's. The end of
/// the first day's heading names whose plays it shows and opens the choice,
/// which the list's count carries; with no plays, the choice sits under the
/// line that says so.
struct HistoryContent: View {
    @Environment(HistoryStore.self)
    private var store
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var retryRead = CancellableTaskSlot()

    var body: some View {
        Group {
            if let list = store.list {
                if let error = list.initialLoadError {
                    MacNotice(
                        title: String(localized: "Could not read your library"),
                        advice: nil,
                        error: error,
                        look: .centered,
                        action: error.retryAction {
                            retryRead.replace { await list.loadInitial() }
                        }
                    )
                }
                else if let count = list.count {
                    let menu = HistoryMenu(count: count)
                    if count.rows == 0 {
                        HistoryEmpty(menu: menu) { uiStore.showHistory(of: $0) }
                    }
                    else {
                        HistoryPlays(
                            list: list,
                            menu: menu.inlineMenu(action: false) {
                                uiStore.showHistory(of: $0)
                            }
                        )
                    }
                }
                else {
                    // The count is a local read; nothing shows until it
                    // lands.
                    Color.clear
                }
            }
            else {
                // The first rows are a local read; nothing shows until they
                // land.
                Color.clear
            }
        }
        .onChange(of: uiStore.historyPerson, initial: true) { _, person in
            store.show(person: person)
        }
        .onDisappear {
            retryRead.cancel()
            store.cancel()
        }
    }
}

/// History's choice of whose plays, as its list's count hands it over: the
/// people it offers and whose plays it shows.
struct HistoryMenu {
    let people: [BridgeHistoryPerson]
    let shown: BridgeHistoryShown

    init(count: BridgeListCount) {
        guard case .history(let people, let shown) = count.extra else {
            preconditionFailure("History's count carried another list's extra")
        }
        self.people = people
        self.shown = shown
    }

    /// Whose plays History shows, in the menu's words.
    private var shownName: String {
        switch shown {
        case .everyone: String(localized: "Everyone")
        case .person(let name): name
        }
    }

    /// The line an empty History says.
    var emptyTitle: String {
        switch shown {
        case .everyone: String(localized: "No plays yet")
        case .person(let name): String(localized: "No plays from \(name)")
        }
    }

    /// The choice of everyone's plays or one person's, which `choose` takes
    /// as the person's member id, or `nil` for everyone's.
    @MainActor
    func inlineMenu(
        action: Bool,
        choose: @escaping (String?) -> Void
    ) -> MacInlineMenu {
        let people = people
        let everyone = shown == .everyone
        return MacInlineMenu(shownName, action: action) {
            MacMenuItem("Everyone", checked: everyone) { choose(nil) }
            for person in people {
                MacMenuItem(verbatim: person.name, checked: person.chosen) {
                    choose(person.person)
                }
            }
        }
    }
}

/// History with no plays from whoever `menu` shows: the line that says so,
/// with the choice of whose plays under it, which `choose` takes as the
/// person's member id, or `nil` for everyone's.
struct HistoryEmpty: View {
    let menu: HistoryMenu
    let choose: (String?) -> Void

    var body: some View {
        MacContentUnavailable(verbatimTitle: menu.emptyTitle) {
            menu.inlineMenu(action: true, choose: choose)
        }
        .frame(maxHeight: .infinity)
    }
}

/// History's plays as rows under their days' headings, each row on screen
/// keeping the list loading it.
private struct HistoryPlays: View {
    let list: HistoryList
    /// The choice of whose plays, which ends the first day's heading.
    let menu: MacInlineMenu

    /// The width of the column the plays' times hang in, left of the covers.
    private static let timeWidth: CGFloat = 56

    @State
    private var fit = ListScreenFit()
    @Environment(\.clockNow)
    private var clockNow

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                ForEach(0..<list.positionCount, id: \.self) { index in
                    row(at: index)
                        .task(
                            id: RowLoadID(epoch: list.loadEpoch, index: index)
                        ) {
                            await list.holdPage(containing: index)
                        }
                        .measuresRow(into: $fit)
                }
            }
            // The margin holds the plays' times, which hang left of the
            // covers' edge.
            .padding(
                .leading,
                ThemeSpace.related + MacListRow.hang(leadWidth: Self.timeWidth)
            )
            .padding(.trailing, ThemeSpace.related)
            .padding(.bottom, ThemeSpace.section)
        }
        .scrollIndicators(.never)
        .measuresScreen(into: $fit)
        .sizesWindow(of: list, from: fit)
    }

    @ViewBuilder
    private func row(at index: Int) -> some View {
        if let play = list.row(at: index) {
            VStack(alignment: .leading, spacing: 0) {
                if play.opensDay {
                    MacQueueSection(
                        title: HistoryWords.day(
                            play.playedOn,
                            now: clockNow ?? Date()
                        ),
                        kind: .day(menu: index == 0 ? menu : nil)
                    )
                }
                MacListRow(
                    lead: HistoryWords.time(play.startedAt),
                    leadWidth: Self.timeWidth,
                    hanging: true,
                    coverSize: 40,
                    cover: play.cover,
                    title: play.title,
                    subtitle: play.artistLine.text,
                    who: play.person,
                    wide: true
                )
            }
        }
        else {
            MacQueuePlaceholderRow()
        }
    }
}

/// How History words a play's time and day, in the person's language.
enum HistoryWords {
    /// The time a play began, as the person's clock shows times.
    static func time(_ startedAt: Int64) -> String {
        Date(timeIntervalSince1970: TimeInterval(startedAt) / 1000)
            .formatted(date: .omitted, time: .shortened)
    }

    /// A day's heading: Today, Yesterday, or the weekday and date.
    static func day(
        _ day: BridgeDay,
        now: Date = .now,
        calendar: Calendar = .current
    ) -> String {
        let components = DateComponents(
            year: Int(day.year),
            month: Int(day.month),
            day: Int(day.day)
        )
        guard let date = calendar.date(from: components) else {
            preconditionFailure("core sent a day no calendar holds: \(day)")
        }
        let today = calendar.startOfDay(for: now)
        if calendar.isDate(date, inSameDayAs: today) {
            return String(localized: "Today")
        }
        guard
            let yesterday = calendar.date(byAdding: .day, value: -1, to: today)
        else {
            preconditionFailure("no calendar holds the day before \(today)")
        }
        if calendar.isDate(date, inSameDayAs: yesterday) {
            return String(localized: "Yesterday")
        }
        return date.formatted(date: .complete, time: .omitted)
    }
}
