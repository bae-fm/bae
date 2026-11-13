import BaeKit
import Testing

@testable import bae

struct WelcomeLibraryRowTests {
    @Test("only a library that can't open as it is has a line under its name")
    func lineUnderTheName() {
        for library in PreviewData.welcomeLibraries {
            switch library.state {
            case .openable:
                #expect(library.stateLine == nil, "\(library.title)")
            case .unreadable, .nameUnreadable, .replacementUnfinished,
                .deletionUnfinished:
                #expect(
                    library.stateLine?.isEmpty == false,
                    "\(library.title)"
                )
            }
        }
        #expect(PreviewData.welcomeLibraries.filter(\.isOpenable).count == 2)
    }

    @Test(
        "a library whose name can't be read is titled by a fixed string with its id under it"
    )
    func unreadableName() throws {
        let library = try #require(
            PreviewData.welcomeLibraries.first {
                if case .nameUnreadable = $0.state { return true }
                return false
            }
        )
        #expect(library.name == nil)
        #expect(
            library.title == localizedCoreString("core.library.name_unreadable")
        )
        #expect(library.stateLine == library.id)
        #expect(!library.isOpenable)
    }
}
