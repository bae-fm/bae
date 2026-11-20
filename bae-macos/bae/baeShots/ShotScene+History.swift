import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The side pane's History tab: Ana's and Ben's plays of today and
    /// yesterday, no plays at all, and no plays from Ben, as the boards draw
    /// it.
    static let historyScenes: [ShotScene] = [
        ShotScene(
            id: "mac.playback.side-history",
            size: CGSize(width: 1200, height: 760)
        ) {
            historyWindow(plays: boardPlays)
        },
        ShotScene(
            id: "mac.playback.side-history.empty",
            size: CGSize(width: 1200, height: 760)
        ) {
            historyWindow(plays: [])
        },
        ShotScene(
            id: "mac.playback.side-history.none-from-person",
            size: CGSize(width: 1200, height: 760)
        ) {
            historyWindow(plays: [], person: benMemberId)
        },
    ]

    /// The shot people: Ana on this Mac, and Ben.
    static let anaMemberId = "4a1f2c3d-0000-4000-8000-000000000001"
    static let benMemberId = "4a1f2c3d-0000-4000-8000-000000000002"

    /// The window with the side pane on History, showing `plays`, the plays
    /// of the person with member id `person` when one is picked.
    private static func historyWindow(
        plays: [BridgeHistoryPlay],
        person: String? = nil
    ) -> ShotStage {
        let uiStore = UiStore()
        uiStore.openSidePane(.history)
        uiStore.showHistory(of: person)
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            history: plays,
            historyPeople: [(anaMemberId, "Ana"), (benMemberId, "Ben")]
        )
        let history = HistoryStore(library: backing.library, onError: { _ in })
        let window = ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: (backing.library, backing.session),
            playbackStore: boardPlayback(),
            history: history,
            size: CGSize(width: 1200, height: 760)
        )
        return ShotStage(window) { history.list != nil }
    }

    /// A play the boards list: its time on `day` days before today, its
    /// track and whose it is.
    private struct BoardPlay {
        let daysAgo: Int
        let hour: Int
        let minute: Int
        let title: String
        let artists: String
        let album: String
        let ana: Bool
    }

    private static let boardPlayList: [BoardPlay] = [
        BoardPlay(
            daysAgo: 0,
            hour: 21,
            minute: 41,
            title: "Harbor Lights",
            artists: "Mara Quill",
            album: "Low Tide Hours",
            ana: true
        ),
        BoardPlay(
            daysAgo: 0,
            hour: 21,
            minute: 37,
            title: "Copper Sky",
            artists: "Odile Fen",
            album: "Copper Sky",
            ana: false
        ),
        BoardPlay(
            daysAgo: 0,
            hour: 21,
            minute: 32,
            title: "Night Ferry",
            artists: "Mara Quill",
            album: "Low Tide Hours",
            ana: true
        ),
        BoardPlay(
            daysAgo: 1,
            hour: 22,
            minute: 14,
            title: "Small Hours",
            artists: "Tomas Brell",
            album: "Small Hours",
            ana: false
        ),
        BoardPlay(
            daysAgo: 1,
            hour: 22,
            minute: 8,
            title: "Paper Lanterns",
            artists: "The Slow Orchards",
            album: "Paper Lanterns",
            ana: true
        ),
    ]

    /// The board's plays as core lists them, on the days and at the times
    /// of this Mac's calendar the boards name, so the headings say Today
    /// and Yesterday.
    @MainActor
    static var boardPlays: [BridgeHistoryPlay] {
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: .now)
        return boardPlayList.enumerated()
            .map { index, play in
                guard
                    let day = calendar.date(
                        byAdding: .day,
                        value: -play.daysAgo,
                        to: today
                    ),
                    let started = calendar.date(
                        bySettingHour: play.hour,
                        minute: play.minute,
                        second: 0,
                        of: day
                    )
                else {
                    preconditionFailure("the calendar holds every board day")
                }
                let date = calendar.dateComponents(
                    [.year, .month, .day],
                    from: day
                )
                let opensDay =
                    index == 0
                    || boardPlayList[index - 1].daysAgo != play.daysAgo
                return BridgeHistoryPlay(
                    id: "play-\(index)",
                    title: play.title,
                    artistLine: .named(play.artists),
                    cover: nil,
                    startedAt: Int64(started.timeIntervalSince1970 * 1000),
                    playedOn: BridgeDay(
                        year: Int32(date.year ?? 0),
                        month: UInt8(date.month ?? 0),
                        day: UInt8(date.day ?? 0)
                    ),
                    opensDay: opensDay,
                    person: BridgePersonRef(
                        person: play.ana ? anaMemberId : benMemberId,
                        name: play.ana ? "Ana" : "Ben",
                        initials: play.ana ? "A" : "B",
                        avatarVersion: nil
                    )
                )
            }
    }
}
