import SwiftUI
import Testing

@testable import bae

@Suite("Line cut")
struct LineCutTests {
    @Test("LineCut cuts a file name in the middle")
    func fileNameIsCutInTheMiddle() {
        #expect(LineCut.fileName.truncation == .middle)
    }

    @Test("LineCut cuts a title at the end")
    func titleIsCutAtTheEnd() {
        #expect(LineCut.prose.truncation == .tail)
    }
}
