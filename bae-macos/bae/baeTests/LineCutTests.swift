import SwiftUI
import Testing

@testable import bae

@Suite("Line cut")
struct LineCutTests {
    @Test("a file name is cut in the middle")
    func fileNameIsCutInTheMiddle() {
        #expect(LineCut.fileName.truncation == .middle)
    }

    @Test("a title is cut at the end")
    func titleIsCutAtTheEnd() {
        #expect(LineCut.prose.truncation == .tail)
    }
}
