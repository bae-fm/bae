import BaeKit
import Testing

@testable import bae

@Suite("Import file evidence")
struct ImportEvidenceTests {
    @Test("a file keeps every extracted value and signal kind")
    func oneFileKeepsAllOfItsEvidence() {
        let evidence = [
            BridgeFileEvidence(
                signal: .barcode,
                value: "5099969394522",
                fileId: "artwork-a.jpg"
            ),
            BridgeFileEvidence(
                signal: .barcode,
                value: "5099969394539",
                fileId: "artwork-a.jpg"
            ),
            BridgeFileEvidence(
                signal: .discId,
                value: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-",
                fileId: "rip.log"
            ),
            BridgeFileEvidence(
                signal: .barcode,
                value: "0602527336459",
                fileId: "artwork-b.jpg"
            ),
        ]

        let found = ImportEvidence.of("artwork-a.jpg", in: evidence)

        #expect(found.map(\.signal) == [.barcode, .barcode])
        let badges = ImportEvidence.badges(found)
        #expect(badges.map(\.signal) == [.barcode])
        #expect(
            badges[0].evidence.map(\.value) == [
                "5099969394522",
                "5099969394539",
            ]
        )
        #expect(!ImportEvidence.hoverText(found).contains("0602527336459"))
        #expect(
            ImportEvidence.hoverText(found).split(separator: "\n").count == 2
        )
        let disc = ImportEvidence.of("rip.log", in: evidence)
        #expect(disc.map(\.signal) == [.discId])
        #expect(disc.map(\.value) == ["XwqRcz4RhAqRTfhE5nRxRKF4iFY-"])
    }
}
