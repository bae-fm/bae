import BaeKit
import SwiftUI

/// The side pane's History tab: the plays this device holds, newest first,
/// under the day they were played, everyone's or one person's. The end of
/// the first day's heading names whose plays it shows and opens the choice;
/// with no plays, the choice sits under the line that says so.
struct HistoryContent: View {
    @Environment(HistoryStore.self)
    private var store
    @Environment(UiStore.self)
    private var uiStore
    @Environment(MembershipStore.self)
    private var membership

    var body: some View {
        Group {
            if let list = store.list {
                if let error = list.initialLoadError {
                    Notice(
                        title: String(localized: "Could not read your library"),
                        advice: nil,
                        error: error,
                        look: .centered,
                        action: nil
                    )
                }
                else if list.count?.rows == 0 {
                    ContentUnavailableView {
                        Text(emptyTitle)
                    } actions: {
                        whoseMenu(action: true)
                    }
                    .frame(maxHeight: .infinity)
                }
                else {
                    HistoryPlays(list: list, menu: whoseMenu(action: false))
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
        .onDisappear { store.cancel() }
    }

    /// Whose plays History shows, in the menu's words.
    private var shownName: String {
        guard let person = uiStore.historyPerson else {
            return String(localized: "Everyone")
        }
        return membership.people.flatMap { Self.name(of: person, in: $0) }
            ?? ""
    }

    private var emptyTitle: String {
        guard uiStore.historyPerson != nil else {
            return String(localized: "No plays yet")
        }
        return String(localized: "No plays from \(shownName)")
    }

    /// The choice of whose plays History shows: everyone's, then each person
    /// with a profile.
    private func whoseMenu(action: Bool) -> MacInlineMenu {
        let people = membership.people.map(Self.named) ?? []
        let chosen = uiStore.historyPerson
        let uiStore = uiStore
        return MacInlineMenu(shownName, action: action) {
            MacMenuItem("Everyone", checked: chosen == nil) {
                uiStore.showHistory(of: nil)
            }
            for person in people {
                MacMenuItem(verbatim: person.name, checked: chosen == person.id)
                {
                    uiStore.showHistory(of: person.id)
                }
            }
        }
    }

    /// The people of `people` with a profile, this device's person first.
    private static func named(_ people: BridgePeople) -> [(
        id: String, name: String
    )] {
        ([people.own].compactMap { $0 } + people.others)
            .compactMap { person in
                person.profile.map { (person.membership.id, $0.name) }
            }
    }

    private static func name(of person: String, in people: BridgePeople)
        -> String?
    {
        named(people).first { $0.id == person }?.name
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
                        title: HistoryWords.day(play.playedOn),
                        menu: index == 0 ? menu : nil,
                        wide: true,
                        hang: true
                    )
                }
                MacListRow(
                    lead: HistoryWords.time(play.startedAt),
                    hanging: true,
                    leadWidth: Self.timeWidth,
                    cover: play.cover,
                    coverSize: 40,
                    title: play.title,
                    subtitle: play.artistLine.text,
                    who: MacListRow.Who(
                        name: play.person.name,
                        initials: play.person.initials,
                        picture: play.person.avatarVersion.map {
                            (play.person.person, $0)
                        }
                    ),
                    wide: true
                )
            }
        }
        else {
            QueuePlaceholderRow()
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
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: today),
            calendar.isDate(date, inSameDayAs: yesterday)
        {
            return String(localized: "Yesterday")
        }
        return date.formatted(date: .complete, time: .omitted)
    }
}
