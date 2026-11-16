import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of playback and of the text fields: the now playing
    /// bar and the controls in it, the queue's rows and headings, and the
    /// labelled text field.
    static let playbackComponentScenes: [ShotScene] =
        nowPlayingBarScenes + seekBarScenes + playPauseScenes
        + queueRowScenes + queueSectionScenes + textFieldScenes
        + trackRowScenes + queuePanelScenes

    /// What the bar's seek bar shows of the track.
    private struct BarLoad {
        var held: [BridgeTrackSpan]
        var waiting = false
        var progress: Double
        var hasPosition = true
    }

    private static func nowPlayingBar(
        _ id: String,
        title: String?,
        playing: Bool,
        load: BarLoad
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 1100, height: 100)
        ) {
            let images = ImageStore(
                fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) }
            )
            return ShotStage(
                NowPlayingBarPreview(
                    trackTitle: title,
                    artistNames: title == nil ? nil : "Mara Quill",
                    cover: title.map {
                        BridgeImageRef(id: $0, version: "1", imageType: .cover)
                    },
                    isPlaying: playing,
                    held: load.held,
                    waitsOnStorage: load.waiting,
                    repeatMode: .off,
                    shuffled: false,
                    hasPosition: load.hasPosition,
                    progress: load.progress
                )
                .environment(images)
                .environment(PreviewData.queueStore(manualCount: 2))
                .environment(Queue.stub())
                .frame(maxHeight: .infinity, alignment: .top),
                images: images
            )
        }
    }

    /// `MacNowPlayingBar`, playing, waiting on the cloud, and with nothing
    /// played.
    private static let nowPlayingBarScenes: [ShotScene] = [
        nowPlayingBar(
            "mac.component.now-playing-bar.playing",
            title: "Low Tide Hours",
            playing: true,
            load: BarLoad(
                held: [BridgeTrackSpan(start: 0, end: 0.6)],
                progress: 102.0 / 303.0
            )
        ),
        nowPlayingBar(
            "mac.component.now-playing-bar.waiting-on-the-cloud",
            title: "Low Tide Hours",
            playing: true,
            load: BarLoad(
                held: [BridgeTrackSpan(start: 0, end: 0.04)],
                waiting: true,
                progress: 5.4 / 303.0
            )
        ),
        nowPlayingBar(
            "mac.component.now-playing-bar.nothing-played",
            title: nil,
            playing: false,
            load: BarLoad(held: [], progress: 0, hasPosition: false)
        ),
    ]

    private static func seekBar(
        _ id: String,
        position: Double,
        held: [BridgeTrackSpan]
    ) -> ShotScene {
        component(id, height: 60) {
            MacSeekBar(
                source: .track(
                    PreviewData.trackPosition(
                        positionMs: Int64((position * 303_000).rounded()),
                        durationMs: 303_000
                    ),
                    buffered:
                        LatestStream.trackBuffered(
                            holding: [
                                "preview-track": BridgeTrackBuffered(
                                    ranges: held
                                )
                            ]
                        )
                        .source(for: "preview-track")
                ),
                showRemainingTime: false,
                onSeek: { _ in },
                onToggleRemainingTime: nil
            )
            .frame(width: 380, height: 20)
        }
    }

    /// `MacSeekBar`, with what is held in one stretch and in two.
    private static let seekBarScenes: [ShotScene] = [
        seekBar(
            "mac.component.seek-bar.playing",
            position: 102.0 / 303.0,
            held: [BridgeTrackSpan(start: 0, end: 0.6)]
        ),
        seekBar(
            "mac.component.seek-bar.holes",
            position: 61.0 / 303.0,
            held: [
                BridgeTrackSpan(start: 0, end: 0.3),
                BridgeTrackSpan(start: 0.55, end: 0.7),
            ]
        ),
    ]

    private static func playPause(
        _ id: String,
        playing: Bool,
        waiting: Bool
    ) -> ShotScene {
        component(id, height: 70) {
            MacPlayPause(
                isPlaying: playing,
                waitsOnStorage: waiting,
                onToggle: {}
            )
        }
    }

    /// `MacPlayPause`: paused, playing, and waiting for the track to arrive.
    private static let playPauseScenes: [ShotScene] = [
        playPause(
            "mac.component.play-pause.paused",
            playing: false,
            waiting: false
        ),
        playPause(
            "mac.component.play-pause.playing",
            playing: true,
            waiting: false
        ),
        playPause(
            "mac.component.play-pause.waiting",
            playing: false,
            waiting: true
        ),
    ]

    private static func queueRow(_ id: String, hover: Bool) -> ShotScene {
        let images = ImageStore(
            fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) }
        )
        return component(id, height: 90, images: images) {
            QueueItemRow(
                item: BridgeQueueEntry(
                    entryId: "e-harbor-lights",
                    trackId: "t-harbor-lights",
                    display: BridgeTrackDisplay(
                        title: "Harbor Lights",
                        artistLine: .named("Mara Quill"),
                        albumId: "a-01",
                        releaseId: "r-01",
                        albumTitle: "Low Tide Hours",
                        coverImage: BridgeImageRef(
                            id: "Low Tide Hours",
                            version: "1",
                            imageType: .cover
                        )
                    ),
                    durationClock: bridgeLengthClock(ms: 272_000)
                ),
                isHovered: hover,
                onHoverChanged: { _ in },
                onSkipTo: { _ in },
                onRemove: { _ in }
            )
            .frame(width: 380, alignment: .leading)
            .environment(images)
        }
    }

    /// `QueueItemRow`, at rest and under the pointer.
    private static let queueRowScenes: [ShotScene] = [
        queueRow("mac.component.queue-row.resting", hover: false),
        queueRow("mac.component.queue-row.hover", hover: true),
    ]

    /// `MacQueueSection`, a lane's heading.
    private static let queueSectionScenes: [ShotScene] = [
        component("mac.component.queue-section.up-next", height: 60) {
            MacQueueSection(
                title: "Up next",
                shuffled: false,
                count: 2,
                clearLabel: "Clear up next",
                onClear: {},
                onSetShuffle: nil
            )
            .frame(width: 380, alignment: .leading)
        },
        component(
            "mac.component.queue-section.playing-from-shuffled",
            height: 60
        ) {
            MacQueueSection(
                title: "Playing from · Low Tide Hours",
                shuffled: true,
                count: 2,
                clearLabel: "Clear playing from",
                onClear: {},
                onSetShuffle: { _ in }
            )
            .frame(width: 380, alignment: .leading)
        },
    ]

    /// How a text field's box stands.
    private enum FieldState {
        case resting, focused, invalid, secret, off
    }

    private static func textField(
        _ id: String,
        note: LocalizedStringKey? = nil,
        text: String = "Home music",
        state: FieldState = .resting
    ) -> ShotScene {
        component(id, height: 90) {
            FieldColumn {
                MacTextField("Name", note: note) {
                    Group {
                        if state == .secret {
                            SecureField("", text: .constant(text))
                        }
                        else {
                            TextField("", text: .constant(text))
                        }
                    }
                    .labelsHidden()
                    .textFieldStyle(.plain)
                    .themeText(.body)
                    .modifier(
                        FieldChrome(
                            focused: state == .focused,
                            style: .boxed,
                            invalid: state == .invalid
                        )
                    )
                    .disabled(state == .off)
                }
            }
            .frame(width: 380, alignment: .leading)
        }
    }

    /// `MacTextField`, a label beside a boxed field, with the note under it.
    private static let textFieldScenes: [ShotScene] = [
        textField("mac.component.text-field.resting"),
        textField("mac.component.text-field.focused", state: .focused),
        textField("mac.component.text-field.empty", text: ""),
        textField(
            "mac.component.text-field.note",
            note: "People in this library see this name."
        ),
        textField(
            "mac.component.text-field.error",
            note: "Enter a name.",
            text: "",
            state: .invalid
        ),
        textField("mac.component.text-field.off", state: .off),
        textField("mac.component.text-field.secret", state: .secret),
    ]

    private static func trackRow(
        _ id: String,
        track: Track,
        artist: String? = nil,
        hover: Bool = false,
        current: Bool = false
    ) -> ShotScene {
        component(id, height: 70) {
            MacTrackRow(
                track: track,
                artist: artist,
                positionWidth: MacTrackRow.leadingSlotMinWidth,
                isCurrent: current,
                isPlaying: current,
                hover: hover,
                onPlay: {},
                onTogglePlayPause: {}
            )
            .frame(width: 380, height: artist == nil ? 40 : 52)
        }
    }

    /// `MacTrackRow`, a track in an album's list.
    private static let trackRowScenes: [ShotScene] = [
        trackRow(
            "mac.component.track-row.resting",
            track: PreviewData.previewTrack(
                title: "Harbor Lights",
                position: "1",
                durationMs: 272_000
            )
        ),
        trackRow(
            "mac.component.track-row.hover",
            track: PreviewData.previewTrack(
                title: "The Long Way Home",
                position: "2",
                durationMs: 312_000
            ),
            hover: true
        ),
        trackRow(
            "mac.component.track-row.current",
            track: PreviewData.previewTrack(
                title: "Second Floor",
                position: "3",
                durationMs: 225_000
            ),
            current: true
        ),
        trackRow(
            "mac.component.track-row.compilation",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000,
                displayArtist: "Mara Quill"
            ),
            artist: "Mara Quill"
        ),
    ]

    private static func queueEntry(
        _ title: String,
        album: String,
        duration: UInt64
    ) -> BridgeQueueEntry {
        BridgeQueueEntry(
            entryId: "e-\(title)",
            trackId: "t-\(title)",
            display: BridgeTrackDisplay(
                title: title,
                artistLine: .named("Mara Quill"),
                albumId: "a-\(album)",
                releaseId: "r-\(album)",
                albumTitle: album,
                coverImage: cover(album)
            ),
            durationClock: bridgeLengthClock(ms: duration)
        )
    }

    /// `MacQueuePanel`, docked at the window's right with a track playing, one
    /// track up next and one playing from the album.
    private static let queuePanelScenes: [ShotScene] = [
        component(
            "mac.component.queue-panel.default",
            width: 436,
            height: 616,
            images: coverStore()
        ) {
            let upcoming = [
                queueEntry(
                    "Tin Roof",
                    album: "Low Tide Hours",
                    duration: 341_000
                )
            ]
            MacQueuePanel(
                manual: [
                    queueEntry(
                        "Margins",
                        album: "Northbound",
                        duration: 295_000
                    )
                ],
                context: QueuePlaybackContext(
                    bridge: BridgePlaybackContext(
                        kind: .release,
                        sourceTitle: "Low Tide Hours"
                    )
                ),
                shuffled: false,
                upcoming: QueueUpcoming(
                    total: upcoming.count,
                    itemAt: {
                        upcoming.indices.contains($0) ? upcoming[$0] : nil
                    },
                    epoch: nil,
                    holdRow: { _ in },
                    failure: nil,
                    onRetry: {}
                ),
                position: PreviewData.trackPosition(
                    positionMs: 102_000,
                    durationMs: 303_000
                ),
                onShowUpcoming: {},
                isActive: true,
                nowPlayingTitle: "Low Tide Hours",
                nowPlayingArtist: "Mara Quill",
                nowPlayingCover: .libraryImage(cover("Low Tide Hours")),
                isPlaying: true,
                onClose: {},
                onGoToNowPlaying: {},
                onPlayPause: {},
                onClearUpNext: {},
                onClearPlayingFrom: {},
                onSkipTo: { _ in },
                onRemove: { _ in },
                onReorder: { _, _, _ in true },
                onReorderFailed: { _ in },
                onInsertTracks: { _, _ in },
                onSetShuffle: { _ in }
            )
        }
    ]
}
