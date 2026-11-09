import BaeKit
import Testing

@testable import bae

struct WelcomeLibraryRowTests {
    @Test("only a library that can't open as it is has a line under its name")
    func lineUnderTheName() {
        for library in PreviewData.welcomeLibraries {
            switch library.state {
            case .openable:
                #expect(library.stateLine == nil, "\(library.name)")
            case .unreadable, .replacementUnfinished, .deletionUnfinished:
                #expect(
                    library.stateLine?.isEmpty == false,
                    "\(library.name)"
                )
            }
        }
        #expect(PreviewData.welcomeLibraries.filter(\.isOpenable).count == 2)
    }
}
