import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of playback and of the text fields: the now playing
    /// bar and the controls in it, the queue's rows and headings, the
    /// labelled text field and the PIN field.
    static let playbackComponentScenes: [ShotScene] =
        nowPlayingBarScenes + transportScenes + volumeSliderScenes
        + seekBarScenes + playPauseScenes + queueRowScenes
        + queueSectionScenes + textFieldScenes + pinFieldScenes
        + trackLeadScenes + trackRowScenes + queuePanelScenes
        + castPopoverScenes + sidePauseScenes

    /// What the bar's seek bar shows of the track.
    private struct BarLoad {
        var held: [BridgeTrackSpan]
        var waiting = false
        var progress: Double
        var hasPosition = true
    }

    /// What the bar's track and its right edge show beside the seek bar.
    private struct BarShow {
        var covered = true
        var casting: BridgeCastReceiver?
        var queueOpen = false
    }

    private static func nowPlayingBar(
        _ id: String,
        title: String?,
        playing: Bool,
        load: BarLoad,
        show: BarShow = BarShow()
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
                    cover: title.flatMap { title in
                        show.covered
                            ? BridgeImageRef(
                                id: title,
                                version: "1",
                                imageType: .cover
                            ) : nil
                    },
                    isPlaying: playing,
                    held: load.held,
                    waitsOnStorage: load.waiting,
                    repeatMode: .off,
                    shuffled: false,
                    hasPosition: load.hasPosition,
                    progress: load.progress,
                    casting: show.casting,
                    queueOpen: show.queueOpen
                )
                .environment(images)
                .environment(PreviewData.queueStore(manualCount: 2))
                .environment(Queue.stub())
                .frame(maxHeight: .infinity, alignment: .top),
                images: images
            )
        }
    }

    /// `MacNowPlayingBar`: playing, waiting on the cloud, with nothing
    /// played, with no cover, casting to a receiver that takes the volume and
    /// to one that does not, and with the queue open.
    private static let nowPlayingBarScenes: [ShotScene] = {
        let playing = BarLoad(
            held: [BridgeTrackSpan(start: 0, end: 0.6)],
            progress: 102.0 / 303.0
        )
        return [
            nowPlayingBar(
                "mac.component.now-playing-bar.playing",
                title: "Low Tide Hours",
                playing: true,
                load: playing
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
            nowPlayingBar(
                "mac.component.now-playing-bar.no-cover",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(covered: false)
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.casting",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(casting: castReceiver("Living Room", .cast))
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.no-volume",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(
                    casting: castReceiver(
                        "Living Room",
                        .dlna,
                        takesVolume: false
                    )
                )
            ),
            nowPlayingBar(
                "mac.component.now-playing-bar.queue-open",
                title: "Low Tide Hours",
                playing: true,
                load: playing,
                show: BarShow(queueOpen: true)
            ),
        ]
    }()

    private static func transport(
        _ id: String,
        playing: Bool = false,
        waiting: Bool = false,
        shuffled: Bool = false,
        repeatMode: BridgeRepeatMode = .off
    ) -> ShotScene {
        component(id, width: 264, height: 60) {
            MacTransport(
                isPlaying: playing,
                waitsOnStorage: waiting,
                shuffled: shuffled,
                repeatMode: repeatMode,
                onPlayPause: {},
                onNext: {},
                onPrevious: {},
                onSetShuffle: { _ in },
                onSetRepeatMode: { _ in }
            )
        }
    }

    /// `MacTransport`: paused, playing, waiting on storage, with shuffle on
    /// and with repeat on.
    private static let transportScenes: [ShotScene] = [
        transport("mac.component.transport.paused"),
        transport("mac.component.transport.playing", playing: true),
        transport(
            "mac.component.transport.waiting",
            playing: true,
            waiting: true
        ),
        transport("mac.component.transport.shuffle", shuffled: true),
        transport("mac.component.transport.repeat", repeatMode: .context),
    ]

    private static func volumeSlider(
        _ id: String,
        _ controls: MacVolumeSlider.Controls
    ) -> ShotScene {
        component(id, width: 184, height: 60) {
            MacVolumeSlider(
                controls: controls,
                onDrag: { _ in },
                onChange: { _ in },
                onSetMuted: { _ in }
            )
        }
    }

    private static func volume(_ level: Float) -> MacVolumeSlider.Controls {
        .controls(
            PlaybackSettings(volume: level, isMuted: false, repeatMode: .off)
        )
    }

    /// `MacVolumeSlider` at 70 %, silent and full, and the line that says
    /// where a receiver without volume sets it.
    private static let volumeSliderScenes: [ShotScene] = [
        volumeSlider("mac.component.volume-slider.resting", volume(0.7)),
        volumeSlider("mac.component.volume-slider.silent", volume(0)),
        volumeSlider("mac.component.volume-slider.full", volume(1)),
        volumeSlider(
            "mac.component.volume-slider.no-volume",
            .setOnReceiver("Living Room")
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
            MacQueueRow(
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
                    durationClock: BridgeDurationClock.length(ms: 272_000)
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

    /// `MacQueueRow`, at rest and under the pointer.
    private static let queueRowScenes: [ShotScene] = [
        queueRow("mac.component.queue-row.resting", hover: false),
        queueRow("mac.component.queue-row.hover", hover: true),
    ]

    /// `MacQueueSection`, a lane's heading.
    private static let queueSectionScenes: [ShotScene] = [
        component("mac.component.queue-section.up-next", height: 60) {
            MacQueueSection(
                title: "Up next",
                kind: .lane(
                    MacQueueSection.Lane(
                        clear: MacQueueSection.Clear(
                            label: "Clear up next",
                            action: {}
                        )
                    )
                )
            )
            .frame(width: 380, alignment: .leading)
        },
        component(
            "mac.component.queue-section.playing-from-shuffled",
            height: 60
        ) {
            MacQueueSection(
                title: "Playing from · Low Tide Hours",
                kind: .lane(
                    MacQueueSection.Lane(
                        clear: MacQueueSection.Clear(
                            label: "Clear playing from",
                            action: {}
                        ),
                        shuffle: MacQueueSection.Shuffle(on: true) { _ in }
                    )
                )
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

    private static func pinField(
        _ id: String,
        digits: Int = 4,
        value: String,
        error: String? = nil,
        focused: Bool = false,
        off: Bool = false
    ) -> ShotScene {
        component(id, height: error == nil ? 80 : 100) {
            MacPinField(
                digits: digits,
                value: .constant(value),
                error: error,
                focusesOnAppear: focused
            )
            .disabled(off)
        }
    }

    /// `MacPinField`, one box per digit of a receiver's PIN.
    private static let pinFieldScenes: [ShotScene] = [
        pinField("mac.component.pin-field.empty", value: ""),
        pinField("mac.component.pin-field.focused", value: "48", focused: true),
        pinField("mac.component.pin-field.filled", value: "4821"),
        pinField(
            "mac.component.pin-field.error",
            value: "4821",
            error: "That PIN did not match."
        ),
        pinField("mac.component.pin-field.off", value: "4821", off: true),
        pinField("mac.component.pin-field.six-digits", digits: 6, value: "482"),
    ]

    private static func trackLead(
        _ id: String,
        current: Bool = false,
        hover: Bool = false
    ) -> ShotScene {
        component(id, width: 84, height: 50) {
            MacTrackLead(
                text: "4",
                isCurrent: current,
                isPlaying: current,
                hover: hover,
                width: MacTrackRow.leadingSlotMinWidth,
                onPlay: {},
                onTogglePlayPause: {}
            )
        }
    }

    /// `MacTrackLead`: the number, Play under the pointer, the speaker on the
    /// current track and Pause under the pointer on it.
    private static let trackLeadScenes: [ShotScene] = [
        trackLead("mac.component.track-lead.number"),
        trackLead("mac.component.track-lead.hover", hover: true),
        trackLead("mac.component.track-lead.current", current: true),
        trackLead(
            "mac.component.track-lead.pause",
            current: true,
            hover: true
        ),
    ]

    private static func trackRow(
        _ id: String,
        track: Track,
        artist: String? = nil,
        hover: Bool = false,
        current: Bool = false,
        picked: Bool = false,
        stars: BridgeStars? = nil
    ) -> ShotScene {
        component(id, height: 70) {
            MacTrackRow(
                track: track,
                artist: artist,
                positionWidth: MacTrackRow.leadingSlotMinWidth,
                isCurrent: current,
                isPlaying: current,
                hover: hover,
                picked: picked,
                stars: stars,
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
            "mac.component.track-row.hover-on-current",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            hover: true,
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
        trackRow(
            "mac.component.track-row.picked",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            picked: true
        ),
        trackRow(
            "mac.component.track-row.picked-and-current",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            current: true,
            picked: true
        ),
        trackRow(
            "mac.component.track-row.rated",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            stars: .four
        ),
        trackRow(
            "mac.component.track-row.rated-and-hover",
            track: PreviewData.previewTrack(
                title: "Low Tide Hours",
                position: "4",
                durationMs: 303_000
            ),
            hover: true,
            stars: .four
        ),
    ]

    /// One of Mara Quill's tracks in the queue, as the boards list it, with
    /// its album's cover unless `covered` is false.
    static func queueEntry(
        _ title: String,
        album: String,
        duration: UInt64,
        covered: Bool = true
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
                coverImage: covered ? cover(album) : nil
            ),
            durationClock: BridgeDurationClock.length(ms: duration)
        )
    }

    /// A queue with a track playing: the lanes it holds, whether the track
    /// plays and whether it has a cover.
    @MainActor
    private struct QueuePlaying {
        let upcoming: [BridgeQueueEntry]
        let manual: [BridgeQueueEntry]
        let isPlaying: Bool
        let covered: Bool

        init(isPlaying: Bool, covered: Bool) {
            upcoming = [
                ShotScene.queueEntry(
                    "Tin Roof",
                    album: "Low Tide Hours",
                    duration: 341_000
                )
            ]
            manual = [
                ShotScene.queueEntry(
                    "Margins",
                    album: "Northbound",
                    duration: 295_000
                )
            ]
            self.isPlaying = isPlaying
            self.covered = covered
        }
    }

    /// The queue panel over what plays, or empty with `nil`.
    private static func queuePanel(
        _ id: String,
        _ queue: QueuePlaying?
    ) -> ShotScene {
        let upcoming = queue?.upcoming ?? []
        let manual = queue?.manual ?? []
        let isPlaying = queue?.isPlaying ?? false
        let isActive = queue != nil
        return component(
            id,
            width: 420,
            height: 616,
            images: coverStore()
        ) {
            MacQueuePanel(tab: .queue, onSelectTab: { _ in }, onClose: {}) {
                QueueTabContent(
                    manual: manual,
                    upNextCount: UInt32(manual.count),
                    isEmpty: !isActive,
                    context: isActive
                        ? QueuePlaybackContext(
                            bridge: BridgePlaybackContext(
                                kind: .release,
                                sourceTitle: "Low Tide Hours"
                            )
                        ) : nil,
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
                    nowPlaying: queue.map(queueNowPlaying),
                    onShowUpcoming: {},
                    isPlaying: isPlaying,
                    onGoToNowPlaying: {},
                    onPlayPause: {},
                    onClearUpNext: {},
                    onClearPlayingFrom: {},
                    onSkipTo: { _ in },
                    onRemove: { _ in },
                    onReorder: { _, _, _ in true },
                    onReorderFailed: { _ in },
                    onInsertTracks: { _, _ in },
                    onDropFailed: { _ in },
                    onSetShuffle: { _ in }
                )
            }
        }
    }

    /// The track `playing` names, as the queue panel's top shows it.
    private static func queueNowPlaying(_ playing: QueuePlaying)
        -> QueueNowPlaying
    {
        QueueNowPlaying(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            cover: playing.covered
                ? .libraryImage(cover("Low Tide Hours")) : nil,
            position: PreviewData.trackPosition(
                positionMs: 102_000,
                durationMs: 303_000
            )
        )
    }

    /// `MacQueuePanel`, docked at the window's right: with a track playing,
    /// paused, with a track that has no cover, and with an empty queue.
    private static let queuePanelScenes: [ShotScene] = [
        queuePanel(
            "mac.component.queue-panel.playing",
            QueuePlaying(isPlaying: true, covered: true)
        ),
        queuePanel(
            "mac.component.queue-panel.paused",
            QueuePlaying(isPlaying: false, covered: true)
        ),
        queuePanel(
            "mac.component.queue-panel.no-cover",
            QueuePlaying(isPlaying: true, covered: false)
        ),
        queuePanel("mac.component.queue-panel.empty", nil),
    ]

    // MARK: - MacCastPopover

    static func castReceiver(
        _ name: String,
        _ kind: BridgeRendererKind,
        takesVolume: Bool = true
    ) -> BridgeCastReceiver {
        BridgeCastReceiver(
            id: name,
            name: name,
            kind: kind,
            model: nil,
            address: "192.168.1.41",
            takesVolume: takesVolume
        )
    }

    private static func castPopover(
        _ id: String,
        casting: BridgeCastReceiver? = nil,
        found: [BridgeCastReceiver] = [],
        connecting: CastStart.Connecting? = nil
    ) -> ShotScene {
        component(id, width: 284, height: 280) {
            MacCastPopover(
                receivers: BridgeCastReceiverList(
                    casting: casting,
                    found: found,
                    isEmpty: casting == nil && found.isEmpty
                ),
                endedFailure: nil,
                onDisconnect: {}
            )
            .environment(
                connecting.map { CastStart.drawn(connecting: $0) }
                    ?? CastStart.stub()
            )
        }
    }

    /// `MacCastPopover`: over a receiver cast to, with long names, with
    /// nothing cast to, while a cast connects, while the receiver's owner
    /// decides, and with no receivers.
    private static let castPopoverScenes: [ShotScene] = [
        castPopover(
            "mac.component.cast-popover.casting",
            casting: castReceiver("Living Room", .cast),
            found: [
                castReceiver("Kitchen", .airPlay),
                castReceiver("Den receiver", .dlna),
            ]
        ),
        castPopover(
            "mac.component.cast-popover.long-names",
            casting: castReceiver("Upstairs Living Room Soundbar", .cast),
            found: [
                castReceiver("Kitchen", .airPlay),
                castReceiver(
                    "Home theater receiver in the basement",
                    .dlna
                ),
            ]
        ),
        castPopover(
            "mac.component.cast-popover.not-casting",
            found: [
                castReceiver("Living Room", .cast),
                castReceiver("Kitchen", .airPlay),
            ]
        ),
        castPopover(
            "mac.component.cast-popover.connecting",
            found: [
                castReceiver("Living Room", .cast),
                castReceiver("Kitchen", .airPlay),
            ],
            connecting: CastStart.Connecting(
                receiver: castReceiver("Kitchen", .airPlay),
                ownerDeciding: false
            )
        ),
        castPopover(
            "mac.component.cast-popover.owner-deciding",
            found: [
                castReceiver("Living Room", .cast),
                castReceiver("Kitchen", .airPlay),
            ],
            connecting: CastStart.Connecting(
                receiver: castReceiver("Kitchen", .airPlay),
                ownerDeciding: true
            )
        ),
        castPopover("mac.component.cast-popover.no-receivers"),
    ]

    // MARK: - MacSidePause

    private static func sidePause(
        _ id: String,
        ended: BridgeSideHeading,
        countdownKey: String?
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 560, height: 340)) {
            let store = PlaybackStore()
            let prompt = BridgeSidePausePrompt(
                id: "side-pause",
                ended: ended,
                countdown: countdownKey.map {
                    BridgeCountdown(
                        key: $0,
                        until: Date().addingTimeInterval(11.95)
                    )
                }
            )
            store.apply(
                .paused(
                    track: BridgeNowPlayingTrack(
                        track: BridgePlayingTrack(
                            trackId: "t-harbor-lights",
                            durationMs: nil
                        ),
                        display: .library(
                            display: BridgeTrackDisplay(
                                title: "Harbor Lights",
                                artistLine: .named("Mara Quill"),
                                albumId: "a-01",
                                releaseId: "r-01",
                                albumTitle: "Low Tide Hours",
                                coverImage: nil
                            )
                        )
                    ),
                    reason: .sideEnded(prompt: prompt)
                )
            )
            return ShotStage(
                Color.clear
                    .background(Theme.surface)
                    .sidePausePromptAlert(showError: { _ in })
                    .environment(store)
                    .environment(Playback.stub())
            )
        }
    }

    /// `MacSidePause`, the card that hangs from the top of the window: the
    /// side's end with the countdown running, the side's end without one, and
    /// the end of a disc.
    private static let sidePauseScenes: [ShotScene] = [
        sidePause(
            "mac.component.side-pause.counting-down",
            ended: .sided(sideLetter: "A"),
            countdownKey: "core.playback.pause.side_ended.countdown"
        ),
        sidePause(
            "mac.component.side-pause.paused",
            ended: .sided(sideLetter: "A"),
            countdownKey: nil
        ),
        sidePause(
            "mac.component.side-pause.disc",
            ended: .disc(disc: 2),
            countdownKey: "core.playback.pause.disc_ended.countdown"
        ),
    ]
}
