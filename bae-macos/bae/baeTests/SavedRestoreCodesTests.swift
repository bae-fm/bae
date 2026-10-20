import BaeKit
import Testing

/// The person's iCloud Keychain as the saved-code list reads and deletes it.
private actor SavedKeychain {
    private var saved: [BridgeSavedRestoreCodeInfo]
    private(set) var deleted: [String] = []
    /// Refuse deletes, as a locked keychain would.
    private var refuseDeletes = false

    init(_ saved: [BridgeSavedRestoreCodeInfo]) {
        self.saved = saved
    }

    func list() -> [BridgeSavedRestoreCode] {
        saved.map { PreviewSavedRestoreCode($0) }
    }

    func forget(_ code: BridgeSavedRestoreCode) throws {
        let libraryId = code.info().libraryId
        if refuseDeletes {
            throw BridgeError.Diagnostic(
                category: .keyringLocked,
                detail: "keychain is unavailable in this session"
            )
        }
        deleted.append(libraryId)
        saved.removeAll { $0.libraryId == libraryId }
    }

    func refuse() {
        refuseDeletes = true
    }
}

private enum SavedCodesTestError: Error {
    case notUsed
}

@MainActor
@Suite("SavedRestoreCodes")
struct SavedRestoreCodesTests {
    private let keychain = SavedKeychain(PreviewSavedRestoreCode.samples)

    private var setup: LibrarySetup {
        let keychain = keychain
        return LibrarySetup(
            listLibraries: { throw SavedCodesTestError.notUsed },
            createLibrary: { throw SavedCodesTestError.notUsed },
            savedRestoreCodes: { await keychain.list() },
            forgetSavedRestoreCode: { try await keychain.forget($0) },
            makeBootstrap: {
                preconditionFailure("the saved-code list starts no restore")
            }
        )
    }

    private func libraryIds(_ codes: SavedRestoreCodes) -> [String] {
        (codes.list.value ?? []).map(\.info.libraryId)
    }

    @Test("lists each saved code under its library's name")
    func listsNames() async {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        #expect(
            (codes.list.value ?? []).map(\.info.name)
                == ["Home Library", "Home Library", "Shared Library"]
        )
    }

    /// Delete asks first; confirming deletes that code from the keychain and
    /// the row goes with it.
    @Test("a confirmed delete removes that code's row")
    func confirmedDeleteRemovesRow() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        let entry = try #require(codes.list.value)[1]

        codes.askToDelete(entry)
        #expect(codes.pendingDelete?.info.libraryId == "saved-02")

        await codes.delete(entry, from: setup)

        #expect(await keychain.deleted == ["saved-02"])
        #expect(codes.pendingDelete == nil)
        #expect(codes.deletingLibraryId == nil)
        #expect(codes.deleteError == nil)
        #expect(libraryIds(codes) == ["saved-01", "saved-03"])
    }

    @Test("dismissing the confirmation deletes nothing")
    func dismissDeletesNothing() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)

        codes.askToDelete(try #require(codes.list.value)[0])
        codes.dismissDelete()

        #expect(codes.pendingDelete == nil)
        #expect(await keychain.deleted.isEmpty)
        #expect(libraryIds(codes) == ["saved-01", "saved-02", "saved-03"])
    }

    /// A refused delete keeps the row, to try again, and says why.
    @Test("a failed delete keeps the row and shows the error")
    func failedDeleteKeepsRow() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        await keychain.refuse()

        await codes.delete(try #require(codes.list.value)[0], from: setup)

        #expect(codes.deleteError != nil)
        #expect(codes.deletingLibraryId == nil)
        #expect(libraryIds(codes) == ["saved-01", "saved-02", "saved-03"])
    }
}
