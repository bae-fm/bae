import BaeKit
import Foundation
import Testing

@MainActor
@Suite("CreateLibraryForm")
struct CreateLibraryFormTests {
    private let host = TestHost.make()

    /// A form whose create reports the creator's name it was given, then
    /// ends as cancelled.
    private func form(
        names: AsyncStream<String>.Continuation
    ) -> CreateLibraryForm {
        let host = host
        return LibrarySetup(
            listLibraries: { [] },
            libraryStanding: { _ in throw CancellationError() },
            createLibrary: { creatorName, _ in
                names.yield(creatorName)
                throw CancellationError()
            },
            savedRestoreCodes: { [] },
            forgetSavedRestoreCode: { _ in throw CancellationError() },
            unfinishedJoins: { [] },
            unfinishedJoinFor: { _ in nil },
            unfinishedRestores: { [] },
            discardUnfinishedInstallation: { _ in throw CancellationError() },
            makeBootstrap: {
                preconditionFailure("a create starts no restore")
            },
            makeSetupSteps: { LibrarySetupSteps(host: host) }
        )
        .createForm(onLibraryReady: { _ in })
    }

    @Test("Create is off while the name is blank")
    func createIsOffWhileBlank() {
        let (_, names) = AsyncStream.makeStream(of: String.self)
        let form = form(names: names)
        #expect(!form.canCreate)
        form.name = "   "
        #expect(!form.canCreate)
        form.name = "Robin"
        #expect(form.canCreate)
    }

    @Test("Create sends the trimmed name")
    func createSendsTheTrimmedName() async {
        let (sent, names) = AsyncStream.makeStream(of: String.self)
        let form = form(names: names)
        form.name = "  Robin \n"
        form.create()
        var iterator = sent.makeAsyncIterator()
        #expect(await iterator.next() == "Robin")
    }

    @Test("A blank name creates nothing")
    func blankCreatesNothing() async {
        let (sent, names) = AsyncStream.makeStream(of: String.self)
        let form = form(names: names)
        form.name = " "
        form.create()
        names.finish()
        var iterator = sent.makeAsyncIterator()
        #expect(await iterator.next() == nil)
    }
}
