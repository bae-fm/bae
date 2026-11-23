import AppKit
import BaeKit
import SwiftUI

@testable import bae

// Fixtures that more than one component's capture file draws with.

extension ShotScene {
    /// Glass Harbor, in the cloud unless `state` and `status` say otherwise.
    static func glassHarborRow(
        _ number: Int,
        state: BridgeReleaseStorageState = .inStorage(pinned: false),
        status: BridgeStorageRowStatus = .inStorage
    ) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Glass Harbor",
                artist: "Ines Varga Trio",
                media: PreviewData.media(.cd),
                state: state,
                status: status,
                files: 9,
                megabytes: 356,
                added: "2026-08-30"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)"
        )
    }

    static func copperSkyRow(_ number: Int) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Copper Sky",
                artist: "Odile Fen",
                media: PreviewData.media(.cd),
                state: .onOneDevice,
                status: .onOneDevice,
                files: 11,
                megabytes: 388,
                added: "2026-10-04"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)"
        )
    }

    /// The three releases the design system's previews list: Copper Sky on
    /// one device, Field Recordings uploading, Glass Harbor in the cloud,
    /// as releases 1 to 3.
    static func threeReleases() -> [BridgeStorageRow] {
        [copperSkyRow(1), fieldRecordingsRow(2), glassHarborRow(3)]
    }

    static func fieldRecordingsRow(_ number: Int) -> BridgeStorageRow {
        storageRow(
            BoardRow(
                title: "Field Recordings",
                artist: "Mara Quill",
                media: PreviewData.media(.cd, 2),
                state: .onOneDevice,
                status: .uploading,
                files: 24,
                megabytes: 902,
                added: "2026-11-06"
            ),
            releaseId: "rel-shot-\(number)",
            albumId: "album-shot-\(number)",
            upload: shotUpload(queued: 0, uploading: 1)
        )
    }

    /// The outbox with the upload of `releaseId`, `fraction` of its 902 MB
    /// sent.
    @MainActor
    static func uploadingOutbox(_ releaseId: String) -> OutboxStore {
        uploadingOutbox(releaseId, fraction: 0.62)
    }

    @MainActor
    static func uploadingOutbox(
        _ releaseId: String,
        fraction: Double
    ) -> OutboxStore {
        OutboxStore(
            uploadsPaused: false,
            releaseProgress: .releaseUploads(
                holding: [
                    releaseId: BridgeReleaseUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: UInt64(902_000_000 * fraction),
                            bytesTotal: 902_000_000,
                            fraction: fraction
                        ),
                        throughputBps: 6_400_000
                    )
                ]
            )
        )
    }

    /// A track as the album panel lists it.
    static func browseTrack(
        _ id: String,
        _ title: String,
        position: String,
        minutes: UInt32,
        seconds: UInt32
    ) -> BridgeTrack {
        PreviewData.previewBridgeTrack(
            id: id,
            title: title,
            position: position,
            durationMs: UInt64(minutes * 60 + seconds) * 1000,
            artist: "Mara Quill"
        )
    }

    static func artistSummary(
        _ name: String,
        albums: UInt64
    ) -> BridgeArtistSummary {
        BridgeArtistSummary(
            artistId: name,
            name: name,
            countLine: BridgeCountLabel(
                key: "core.library.album_count",
                count: albums
            ),
            image: cover(name)
        )
    }

    /// The four images the lightbox's strip lists.
    static let lightboxImages = [
        "Low Tide Hours", "Low Tide Hours back", "Low Tide Hours disc",
        "Low Tide Hours booklet",
    ]

    static func lightboxItems() -> [LightboxItem] {
        lightboxImages.map {
            LightboxItem(
                id: $0,
                label: "cover.jpg",
                image: .libraryImage(cover($0))
            )
        }
    }

    static func playlistSummary(
        owner: String,
        sharing: BridgeSharing,
        yours: Bool
    ) -> BridgePlaylistSummary {
        BridgePlaylistSummary(
            id: "playlist-late-night",
            name: "Harbor evenings",
            sharing: sharing,
            owner: BridgePersonRef(
                person: "person-\(owner)",
                name: owner,
                initials: String(owner.prefix(1)),
                avatarVersion: nil
            ),
            yours: yours,
            headerSharing: yours ? sharing : nil,
            trackCount: BridgeCountLabel(
                key: "core.library.track_count",
                count: 12
            ),
            totalDuration: .minutesOnly(minutes: 54),
            cover: .grid(
                covers: [
                    cover("Low Tide Hours"), cover("Paper Lanterns"),
                    cover("Glass Harbor"), cover("Copper Sky"),
                ]
            )
        )
    }

    /// Storage turning this Mac's sign-in down, which sync reports as its
    /// failure.
    static let syncRefused = BridgeError.Diagnostic(
        category: .storage(failure: .authentication),
        detail: "Dropbox answered 401: expired_access_token"
    )

    /// The store the Activity panel and button read: `activity`, the bars of
    /// its lines at `bars`, and the failure to read it when `readError`.
    @MainActor
    static func activityStore(
        _ activity: BridgeSyncActivity?,
        bars: [String: Double] = [:],
        readError: (any Error)? = nil
    ) -> SyncActivityStore {
        let store = SyncActivityStore(
            activity: activity,
            bars: .activityBars(holding: bars)
        )
        if let readError {
            store.fail(readError)
        }
        return store
    }

    /// What the title bar's activity button reads while bytes move, while a
    /// problem is listed and while a queue is held: the button's scenes and
    /// the title bar's draw the same three.
    static let movingActivity = BridgeSyncActivity(
        moving: true,
        held: false,
        failure: nil,
        syncing: true,
        uploads: nil,
        pins: nil,
        movingToThisDevice: nil
    )

    static let attentionActivity = BridgeSyncActivity(
        moving: false,
        held: false,
        failure: syncRefused,
        syncing: false,
        uploads: nil,
        pins: nil,
        movingToThisDevice: nil
    )

    static let pausedActivity = BridgeSyncActivity(
        moving: false,
        held: true,
        failure: nil,
        syncing: false,
        uploads: nil,
        pins: BridgeDownloadQueueActivity(
            batch: BridgeTransferBatch(done: 1, total: 3),
            wait: .paused,
            paused: true
        ),
        movingToThisDevice: nil
    )

    /// What the import views read from the environment, with the stand-ins
    /// the other import shots use.
    static func importStores<Content: View>(
        _ content: Content,
        syncs: Bool = false
    ) -> some View {
        content
            .environment(ImportStore())
            .importPreviewStores(syncs: syncs)
    }

    /// One track of the mapping table: the file it plays from, its title
    /// and the position core numbers it by. By default the album's artists
    /// are its artists. `artist` is the artist cell the row draws. `listedSeconds` is
    /// the length the source lists when it differs from the audio's.
    static func mappingTrack(
        file: String,
        position: String,
        title: String,
        seconds: UInt64,
        artists: BridgeTrackArtistAssignments = .albumArtists,
        listedSeconds: UInt64? = nil,
        artist: BridgeTrackArtistDisplay = .albumArtist
    ) -> BridgeTrackTableRow {
        BridgeTrackTableRow(
            track: BridgeRawTrackEdit(
                id: "component-\(file)",
                title: title,
                artistAssignments: artists,
                file: .standalone(fileId: file)
            ),
            source: .file(name: file),
            audition: PreviewData.wholeFile(file),
            position: position,
            length: BridgeDurationClock.length(ms: seconds * 1_000),
            listedLength: listedSeconds.map {
                BridgeDurationClock.length(ms: $0 * 1_000)
            },
            artist: artist
        )
    }

    static func previewTarget(_ name: String) -> BridgePreviewTarget {
        BridgePreviewTarget(
            path: Data(name.utf8),
            name: name,
            startSample: 0,
            endSample: nil
        )
    }

    /// The pictures of the tiles and the preview: every image file is the
    /// release's cover.
    static func coverImages() -> ImageStore {
        ImageStore(
            fetchLibraryImageBytes: { try BoardArtwork.data(named: $0.id) },
            fetchReleaseImageBytes: { _, _ in
                try BoardArtwork.data(named: "cover.jpg")
            },
            fetchRemoteImage: { image, _ in
                try BoardArtwork.data(named: image.url)
            }
        )
    }

    /// The gallery's entry for the image file `name` of the release.
    static func coverEntry(_ name: String) -> BridgeGalleryCover {
        BridgeGalleryCover(
            key: .file(fileId: name),
            image: .releaseImage(
                releaseId: "rel-board-0",
                source: .releaseFile(fileId: name)
            ),
            label: .file(
                name: name,
                selection: .releaseImage(fileId: name)
            )
        )
    }

    /// A draft that names nothing yet.
    static func blankEdit() -> BridgeRawReleaseEdit {
        PreviewData.blankDraft(files: [])
    }

    /// Mara Quill as a credit the library has no artist for, as the
    /// pane's read answers the field: the artist is new.
    static let newArtists = BridgeArtistField(
        standing: .new,
        credits: [
            BridgeStandingCredit(
                credited: PreviewData.artistCredit("Mara Quill"),
                standing: .new
            )
        ]
    )

    /// The library album the records name.
    static let libraryAlbum = BridgeDraftAlbum.named(
        albumId: "album-low-tide-hours",
        line: .titleAndArtists(
            title: "Low Tide Hours",
            artistLine: .named("Mara Quill")
        )
    )

    static let inertAlbumActions = DraftAlbumActions(
        pick: { _ in },
        search: { _ in [] }
    )

    static func artistSearch() -> ArtistSearchOpener {
        PreviewData.artistAssignmentsLibrary().artistSearch
    }

    static func pressing(
        label: String = "Harbor Records",
        catalog: String = "HR-1042",
        barcode: String? = nil,
        packaging: BridgePackaging? = nil,
        details: [BridgeDiscogsDetail] = []
    ) -> BridgeRawPressingEdit {
        BridgeRawPressingEdit(
            year: "1992",
            labels: [
                .alone(name: label, catalogNumber: catalog)
            ],
            barcodes: barcode.map { [.alone($0)] } ?? [],
            facts: PreviewData.pressingFacts(
                country: "US",
                media: PreviewData.media(.cd),
                status: .official,
                packaging: packaging,
                discogsDetails: details
            )
        )
    }

    /// The id a catalog gives its record of the pressing `releaseId`.
    static func recordId(
        _ releaseId: String,
        _ catalog: BridgeLookupCatalog
    ) -> String {
        "\(releaseId)-\(catalog == .musicBrainz ? "mb" : "discogs")"
    }

    /// The record `source` holds of the pressing `releaseId`.
    static func record(
        _ source: BridgeLookupCatalog,
        releaseId: String,
        year: Int32?,
        facts: BridgePressingFacts,
        barcode: String?
    ) -> BridgeMetadataResult {
        BridgeMetadataResult(
            source: source,
            releaseId: recordId(releaseId, source),
            year: year,
            facts: facts,
            barcodes: barcode.map { [$0] } ?? [],
            sourceGroupId: "component-low-tide-hours"
        )
    }

    /// What picking the pressing `releaseId` links the folder to.
    static func pick(
        _ releaseId: String,
        _ source: BridgeLookupCatalog
    ) -> BridgePressingLink {
        BridgePressingLink(
            record: BridgeLookupRef(
                catalog: source,
                key: recordId(releaseId, source)
            ),
            partners: []
        )
    }

    /// A pressing of Low Tide Hours as the catalogs `sources` list it, whose
    /// records agree: its year, Harbor Records and the catalog number, in
    /// the format `facts` states.
    static func pressing(
        releaseId: String,
        year: Int32?,
        catalog: String?,
        facts: BridgePressingFacts,
        sources: [BridgeLookupCatalog],
        barcode: String? = nil,
        chosen: Bool = false,
        inLibrary: Bool = false
    ) -> BridgePressing {
        BridgePressing(
            releases: sources.map {
                record(
                    $0,
                    releaseId: releaseId,
                    year: year,
                    facts: facts,
                    barcode: barcode
                )
            },
            sources: sources,
            labels: [
                BridgeLabelLine(
                    names: ["Harbor Records"],
                    catalogNumbers: catalog.map { [$0] } ?? []
                )
            ],
            pick: pick(releaseId, sources[0]),
            chosen: chosen,
            inLibrary: inLibrary
        )
    }

    /// The 1992 CD, which both catalogs list and the folder's Disc ID and
    /// barcode agree with.
    static func cdPressing(
        chosen: Bool = false,
        inLibrary: Bool = false,
        barcode: String? = "0 41234 56789 2"
    ) -> BridgePressing {
        pressing(
            releaseId: "hr-1042",
            year: 1992,
            catalog: "HR-1042",
            facts: PreviewData.pressingFacts(
                country: "US",
                media: PreviewData.media(.cd)
            ),
            sources: [.musicBrainz, .discogs],
            barcode: barcode,
            chosen: chosen,
            inLibrary: inLibrary
        )
    }

    /// What the folder's own text agrees with about a pressing.
    static func agreements(
        catalog: Bool = false,
        notes: [BridgeTextStatement] = []
    ) -> BridgeAgreements {
        BridgeAgreements(
            discId: true,
            barcode: true,
            catalog: catalog,
            label: false,
            year: false,
            country: false,
            notes: notes
        )
    }

    static let sampleBarcode = "0 41234 56789 2"

    /// The album's cover, which the catalog serves under the album's title.
    static let lowTideHoursCover: BridgeRemoteCover = {
        let image = BridgeRemoteImageSet(url: "Low Tide Hours", downscaled: [])
        return BridgeRemoteCover(
            coverChoice: PreviewData.catalogCoverChoice(
                image,
                source: .musicBrainz
            ),
            sourceName: "Cover Art Archive",
            label: .lead(releaseGroup: true)
        )
    }()

    /// Low Tide Hours by Mara Quill on Harbor Records, as the catalogs
    /// `sources` list it, with `pressings` under it.
    static func lowTideHoursGroup(
        titles: [String] = ["Low Tide Hours"],
        sources: [BridgeCatalog] = [.musicBrainz, .discogs],
        pressings: [BridgePressing] = []
    ) -> ReleaseGroup {
        ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "component-low-tide-hours",
                titles: titles,
                artists: [.named("Mara Quill")],
                labels: ["Harbor Records"],
                coverArt: lowTideHoursCover,
                sources: sources.map {
                    BridgeReleaseGroupSource(
                        source: $0,
                        groupUrl: $0 == .musicBrainz
                            ? "https://musicbrainz.org/release-group/component-low-tide-hours"
                            : "https://www.discogs.com/master/component-low-tide-hours",
                        albumLinksUnread: false
                    )
                },
                sections: [
                    BridgePressingSection(
                        album: nil,
                        pressings: pressings,
                        narrowedOut: []
                    )
                ]
            )
        )
    }

    /// The preview's wide column.
    static let wideColumn: CGFloat = 842

    /// What the list's column shows beyond its two rows: the text typed and
    /// the count it narrows to, the work that runs, and the filter that is
    /// on.
    struct ColumnState {
        var tab: BridgeTriageTab = .pending
        var typed = ""
        var narrowed: BridgeNarrowedCount?
        var identifying = false
        var importing = false
        var scanning = false
        var filtered = false
        var items: () throws -> [BridgeImportListItem] = {
            PreviewData.lookupItems(images: try PreviewData.LookupBoardImages())
        }
    }

    static let columnCounts = BridgeTriageTabCounts(
        pending: 14,
        done: 31,
        skipped: 2
    )

    /// The store the column reads: a watched folder, and the work `state`
    /// says runs.
    @MainActor
    static func columnStore(_ state: ColumnState) -> ImportStore {
        let store = ImportStore()
        store.applySummary(
            PreviewData.importQueueSummary(
                hasWatchedFolders: true,
                hasGroups: false
            )
        )
        if state.identifying {
            store.identificationProgress = (identified: 4, total: 10)
        }
        if state.importing {
            store.importsInFlight = 2
        }
        if state.scanning {
            store.applyFolderScans(
                BridgeFolderScans(
                    statuses: [],
                    hasProblem: false,
                    scanning: true,
                    alerts: []
                )
            )
        }
        return store
    }

    static func coverStore() -> ImageStore {
        ImageStore(
            fetchLibraryImageBytes: { try BoardArtwork.data(named: $0.id) }
        )
    }

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

    static func storageState(
        _ id: String,
        _ state: BridgeReleaseStorageState
    ) -> ShotScene {
        component(id, height: 40) {
            MacStorageState(state: state)
        }
    }

    /// A release's upload with `queued` files waiting and `uploading` moving.
    static func shotUpload(
        queued: UInt32,
        uploading: UInt32,
        missing: String? = nil
    ) -> BridgeReleaseUpload {
        BridgeReleaseUpload(
            progress: BridgeUploadProgress(
                queued: queued,
                uploading: uploading,
                activity: uploading > 0 ? .uploading : .queued
            ),
            summary: [],
            missing: missing
        )
    }

    static let moveFailure = "The network connection was lost."

    /// A file of a release, named `name`: audio when it has an `audio`
    /// format, else the `kind` it names.
    static func releaseFile(
        _ name: String,
        audio: BridgeAudioFormat? = nil,
        size: UInt64,
        kind: BridgeFileKind? = nil,
        contentType: String = "application/octet-stream"
    ) -> BridgeFile {
        BridgeFile(
            id: name,
            originalFilename: name,
            name: name,
            folder: nil,
            kind: kind ?? (audio == nil ? .other : .audio(format: "FLAC")),
            fileSize: size,
            contentType: contentType,
            isImage: false,
            audioFormat: audio
        )
    }

    static let leavingLibrary = BridgeClosingProgress(
        step: .leavingLibrary,
        key: "core.library.closing_step.leaving_library"
    )

    /// Home music's open failing, in a way a reset from the cloud can fix.
    static let homeMusicOpenFailure = LoadError(
        .libraryOpen(offersReset: true),
        PreviewData.displayErrorWithDetail,
        libraryId: PreviewData.homeMusic.id
    )

    /// A library whose storage settings bae cannot read.
    static let unreadableLibrary = BridgeLibrary(
        id: "lib-garden",
        name: "Garden",
        path: "/Users/preview/Music/bae/Garden",
        state: .unreadable(
            error: .Diagnostic(
                category: .internal,
                detail: "storage.json: expected value at line 1"
            )
        ),
        isActive: false
    )
}

/// A Storage Manager list read from `store` and drawn by `content` once its
/// first rows land, as the window does.
struct ShotStorageList<Content: View>: View {
    let store: StorageManagerStore
    @ViewBuilder
    let content: @MainActor (StorageManagerStore, StorageList) -> Content

    var body: some View {
        Group {
            if let list = store.list {
                content(store, list)
            }
        }
        .task {
            store.update(
                filter: .all,
                sort: BridgeStorageSort(
                    field: .albumTitle,
                    direction: .ascending
                ),
                search: ""
            )
        }
    }
}

extension View {
    /// The stores a transfer line's pause button reads.
    func transferLineStores() -> some View {
        environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(UiStore())
    }
}
