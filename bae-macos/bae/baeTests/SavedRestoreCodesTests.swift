import BaeKit
import Testing

/// The person's iCloud Keychain as the saved-code list reads and deletes it:
/// codes bae can restore from, then entries it can't read.
private actor SavedKeychain {
    private var saved: [BridgeSavedRestoreCodeInfo]
    private var unreadable:
        [(libraryId: String, reason: BridgeUnreadableSavedCode)]
    private(set) var deleted: [String] = []
    /// Refuse deletes, as a locked keychain would.
    private var refuseDeletes = false

    init(
        _ saved: [BridgeSavedRestoreCodeInfo],
        unreadable: [(libraryId: String, reason: BridgeUnreadableSavedCode)]
    ) {
        self.saved = saved
        self.unreadable = unreadable
    }

    func list() -> [BridgeSavedRestoreEntry] {
        saved.map { .readable(code: PreviewSavedRestoreCode($0)) }
            + unreadable.map {
                .unreadable(libraryId: $0.libraryId, reason: $0.reason)
            }
    }

    func forget(_ libraryId: String) throws {
        if refuseDeletes {
            throw BridgeError.Diagnostic(
                category: .keyringLocked,
                detail: "keychain is unavailable in this session"
            )
        }
        deleted.append(libraryId)
        saved.removeAll { $0.libraryId == libraryId }
        unreadable.removeAll { $0.libraryId == libraryId }
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
    private let keychain = SavedKeychain(
        PreviewSavedRestoreCode.samples,
        unreadable: [(libraryId: "saved-04", reason: .format)]
    )

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
        (codes.list.value ?? []).map(\.libraryId)
    }

    @Test("lists each saved code under its library's name")
    func listsNames() async {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        let names = (codes.list.value ?? [])
            .compactMap { entry -> String? in
                guard case .readable(let saved) = entry else { return nil }
                return saved.info.name
            }
        #expect(names == ["Home Library", "Home Library", "Shared Library"])
    }

    /// An entry bae can't read is listed with its library id and why, and
    /// carries no code a restore could start from.
    @Test("lists an unreadable entry beside the readable codes")
    func listsUnreadable() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        let entries = try #require(codes.list.value)
        #expect(entries.count == 4)
        guard case .unreadable(let libraryId, let reason) = entries[3] else {
            Issue.record("the fourth entry should be unreadable")
            return
        }
        #expect(libraryId == "saved-04")
        #expect(reason == .format)
    }

    /// Deleting an unreadable entry asks first, as a readable code's does,
    /// then deletes it by its library id.
    @Test("a confirmed delete removes an unreadable entry's row")
    func confirmedDeleteRemovesUnreadableRow() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        let entry = try #require(codes.list.value)[3]

        codes.askToDelete(entry)
        #expect(codes.pendingDelete?.libraryId == "saved-04")

        await codes.delete(entry, from: setup)

        #expect(await keychain.deleted == ["saved-04"])
        #expect(codes.pendingDelete == nil)
        #expect(codes.deleteError == nil)
        #expect(libraryIds(codes) == ["saved-01", "saved-02", "saved-03"])
    }

    /// Delete asks first; confirming deletes that code from the keychain and
    /// the row goes with it.
    @Test("a confirmed delete removes that code's row")
    func confirmedDeleteRemovesRow() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)
        let entry = try #require(codes.list.value)[1]

        codes.askToDelete(entry)
        #expect(codes.pendingDelete?.libraryId == "saved-02")

        await codes.delete(entry, from: setup)

        #expect(await keychain.deleted == ["saved-02"])
        #expect(codes.pendingDelete == nil)
        #expect(codes.deletingLibraryId == nil)
        #expect(codes.deleteError == nil)
        #expect(libraryIds(codes) == ["saved-01", "saved-03", "saved-04"])
    }

    @Test("dismissing the confirmation deletes nothing")
    func dismissDeletesNothing() async throws {
        let codes = SavedRestoreCodes()
        await codes.load(from: setup)

        codes.askToDelete(try #require(codes.list.value)[0])
        codes.dismissDelete()

        #expect(codes.pendingDelete == nil)
        #expect(await keychain.deleted.isEmpty)
        #expect(
            libraryIds(codes) == [
                "saved-01", "saved-02", "saved-03", "saved-04",
            ]
        )
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
        #expect(
            libraryIds(codes) == [
                "saved-01", "saved-02", "saved-03", "saved-04",
            ]
        )
    }
}
