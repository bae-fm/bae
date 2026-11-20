import BaeKit
import Foundation
import Testing

@testable import bae

/// A list of plays whose view answers only once the test opens it, and
/// what each History read asked for.
private final class HistoryReads: @unchecked Sendable {
    private let lock = NSLock()
    private var asked: [String?] = []
    private let held = AsyncStream<Void>.makeStream()

    var people: [String?] { lock.withLock { asked } }

    /// A library whose History answers everyone's plays with `everyone` at
    /// once, and a person's with `theirs` once `release` runs.
    func library(
        everyone: [BridgeHistoryPlay],
        theirs: [BridgeHistoryPlay]
    ) -> Library {
        Library(historyList: { [self] person in
            lock.withLock { asked.append(person) }
            guard person != nil else {
                return .fixed(everyone, answers: UnansweredCount())
            }
            return heldList(theirs)
        })
    }

    /// Let the held list answer its first rows.
    func release() {
        held.continuation.yield()
    }

    private func heldList(
        _ rows: [BridgeHistoryPlay]
    ) -> BridgeListQueries<BridgeHistoryPlay> {
        let stream = held.stream
        let fixed = BridgeListQueries<BridgeHistoryPlay>
            .fixed(rows, answers: UnansweredCount())
        return BridgeListQueries(
            view: { first, last in
                ListViewQuery(
                    setVisible: { _, _ in },
                    values: PulledValues(read: {
                        for await _ in stream {
                            return BridgeListQueries.fixedDelivery(
                                first: Int(first),
                                last: Int(last),
                                rows: rows
                            )
                        }
                        throw CancellationError()
                    })
                )
            },
            count: fixed.count
        )
    }
}

private func play(_ title: String, by person: String) -> BridgeHistoryPlay {
    BridgeHistoryPlay(
        id: "play-\(title)",
        title: title,
        artistLine: .named("Artist Name"),
        cover: nil,
        startedAt: 0,
        playedOn: BridgeDay(year: 2026, month: 10, day: 18),
        opensDay: true,
        person: BridgePlaylistOwner(
            person: person,
            name: person,
            initials: String(person.prefix(1)),
            avatarVersion: nil
        )
    )
}

@Suite("HistoryStore")
struct HistoryStoreTests {
    @MainActor
    @Test(
        "a new choice of whose plays opens a new read, and the last plays stay until it answers"
    )
    func theLastPlaysStayUntilTheNewReadAnswers() async throws {
        let reads = HistoryReads()
        let store = HistoryStore(
            library: reads.library(
                everyone: [play("One", by: "ana"), play("Two", by: "ben")],
                theirs: [play("Two", by: "ben")]
            ),
            onError: { Issue.record("Read failed: \($0)") }
        )

        store.show(person: nil)
        try await Wait.until { store.list?.count?.rows == 2 }
        let everyone = store.list

        store.show(person: "ben")
        #expect(reads.people == [nil, "ben"])
        #expect(store.list === everyone)

        reads.release()
        try await Wait.until { store.list !== everyone }
        #expect(store.list?.count?.rows == 1)
        store.cancel()
        #expect(store.list == nil)
    }
}

@Suite("History's words")
struct HistoryWordsTests {
    private let calendar = Calendar(identifier: .gregorian)

    @Test("a day's heading says Today, Yesterday, or the weekday and date")
    func dayHeadings() throws {
        let now = try #require(
            calendar.date(
                from: DateComponents(year: 2026, month: 10, day: 18, hour: 9)
            )
        )
        #expect(
            HistoryWords.day(
                BridgeDay(year: 2026, month: 10, day: 18),
                now: now,
                calendar: calendar
            ) == String(localized: "Today")
        )
        #expect(
            HistoryWords.day(
                BridgeDay(year: 2026, month: 10, day: 17),
                now: now,
                calendar: calendar
            ) == String(localized: "Yesterday")
        )
        let older = try #require(
            calendar.date(from: DateComponents(year: 2026, month: 10, day: 12))
        )
        #expect(
            HistoryWords.day(
                BridgeDay(year: 2026, month: 10, day: 12),
                now: now,
                calendar: calendar
            ) == older.formatted(date: .complete, time: .omitted)
        )
    }
}
