#if DEBUG
    import BaeKit
    import SwiftUI

    /// A fixed bridge reply; the review, layout, and controls are production views.
    final class ImportCombinationPreviewReply:
        CandidateCombinationReviewProtocol, Sendable
    {
        static let keys = [
            "/Music/Incoming/Volume One", "/Music/Incoming/Volume Two",
        ]
        static let value = BridgeCombinationPreview(
            parts: [
                BridgeCombinationPart(
                    candidateKey: keys[0],
                    folderName: "Volume One",
                    filePrefix: "01 - Volume One/",
                    firstDisc: 1,
                    discCount: 1,
                    trackCount: 2
                ),
                BridgeCombinationPart(
                    candidateKey: keys[1],
                    folderName: "Volume Two",
                    filePrefix: "02 - Volume Two/",
                    firstDisc: 2,
                    discCount: 1,
                    trackCount: 2
                ),
            ],
            tracks: [
                BridgeTrackUserEdit(
                    title: "Opening",
                    side: 1,
                    trackNumber: 1,
                    artistAssignments: .albumArtists,
                    file: .standalone(fileId: "01 - Volume One/01 Opening.flac")
                ),
                BridgeTrackUserEdit(
                    title: "Crossing",
                    side: 1,
                    trackNumber: 2,
                    artistAssignments: .albumArtists,
                    file: .standalone(
                        fileId: "01 - Volume One/02 Crossing.flac"
                    )
                ),
                BridgeTrackUserEdit(
                    title: "Return",
                    side: 2,
                    trackNumber: 1,
                    artistAssignments: .albumArtists,
                    file: .standalone(fileId: "02 - Volume Two/01 Return.flac")
                ),
                BridgeTrackUserEdit(
                    title: "Closing",
                    side: 2,
                    trackNumber: 2,
                    artistAssignments: .albumArtists,
                    file: .standalone(fileId: "02 - Volume Two/02 Closing.flac")
                ),
            ]
        )

        func candidateKeys() -> [String] { Self.keys }
        func preview(keys: [String], order: BridgeCombinationTrackOrder) throws
            -> BridgeCombinationPreview
        {
            guard keys == Self.keys, order == .separateDiscs else {
                throw StubError.notImplemented
            }
            return Self.value
        }
        func combine(
            keys: [String],
            order: BridgeCombinationTrackOrder,
            name: String
        ) async throws -> String {
            throw StubError.notImplemented
        }
    }

    @MainActor
    struct ImportCombinationPreviewScene: View {
        @State
        private var review: ImportCombinationReview

        init() {
            do {
                let review = try ImportCombinationReview(
                    review: ImportCombinationPreviewReply()
                )
                review.name = "Collected Volumes"
                _review = State(initialValue: review)
            }
            catch {
                fatalError(
                    "The fixed combination preview must provide its initial reply: \(error)"
                )
            }
        }

        var body: some View {
            ImportCombinationReviewView(
                review: review,
                onCancel: {},
                onCombined: { _ in }
            )
            .windowBackground()
        }
    }

    #Preview("Combine selected folders") { ImportCombinationPreviewScene() }
#endif
