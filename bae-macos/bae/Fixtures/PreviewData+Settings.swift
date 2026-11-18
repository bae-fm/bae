import BaeKit
import Foundation

@testable import bae

// Fixtures for the settings flows — the save-format presets and
// the filename pattern, plus the Library people (this device's person as
// admin plus one removable member), a `Sync` whose reads serve shot
// values, and a connected config store. Generic placeholder identities
// throughout.
extension PreviewData {
    // MARK: - Membership

    static let previewStorage = BridgeStorageLocation.s3(
        bucket: "preview-bucket",
        region: "us-east-1",
        endpoint: nil,
        profile: nil
    )

    /// A Dropbox shared folder, with the account and the folder's label
    /// coven read at the latest connection.
    static let previewDropboxStorage = BridgeStorageLocation.dropbox(
        location: .sharedFolder(namespaceId: "1234567890"),
        accountLabel: "name@example.com",
        folderLabel: BridgeFolderLabel(
            name: "Music",
            webUrl: "https://www.dropbox.com/home/Music"
        )
    )

    /// Returns two people who reach `storage` as its members do: by an S3
    /// key on S3, by their own account on Dropbox, OneDrive and iCloud.
    /// This device's person is an admin with this Mac and a phone. Their
    /// former devices are an old Mac, which this Mac's new registration
    /// replaced, and a removed old phone. The other member has a removable
    /// tablet and two removed devices. Ids are arbitrary UUIDs. The counts
    /// are two people and three devices, as core counts them: the Mac and
    /// the phone, and the other member's tablet.
    static func members(reaching storage: BridgeStorageLocation)
        -> BridgePeople
    {
        let reach: (BridgePerson) -> BridgePerson =
            switch storage {
            case .dropbox, .oneDrive, .cloudKit: { $0 }
            case .s3:
                { person in
                    var person = person
                    person.membership.access = .s3AccessKey(
                        accessKeyId: "AKIAPREVIEWKEY"
                    )
                    return person
                }
            }
        return BridgePeople(
            own: reach(accountSelf),
            others: [reach(accountOther)],
            people: 2,
            devices: 3
        )
    }

    /// An empty `BridgePeople`, with no people and no devices, for fixtures
    /// that show no people.
    static let noPeople = BridgePeople(
        own: nil,
        others: [],
        people: 0,
        devices: 0
    )

    /// This device's person in `members`, reaching the library by their own
    /// account.
    private static let accountSelf =
        BridgePerson(
            profile: BridgePersonProfile(
                id: "4a1f2c3d-0000-4000-8000-000000000001",
                name: "Ana",
                historySharing: .private,
                avatarVersion: nil
            ),
            membership: BridgeMember(
                id: "4a1f2c3d-0000-4000-8000-000000000001",
                role: .admin,
                access: .providerAccount(email: "person-a@example.com"),
                isSelf: true,
                canRemove: false,
                activeDevices: [
                    BridgeActiveDevice(
                        id: 3,
                        name: "This Mac",
                        isSelf: true,
                        canRemove: false
                    ),
                    BridgeActiveDevice(
                        id: 6,
                        name: "Phone",
                        isSelf: false,
                        canRemove: true
                    ),
                ],
                formerDevices: [
                    BridgeFormerDevice(
                        id: 1,
                        name: "Old Mac",
                        state: .replaced
                    ),
                    BridgeFormerDevice(
                        id: 7,
                        name: "Old phone",
                        state: .removed
                    ),
                ],
                activeDeviceCount: 2,
                formerDeviceCount: 2
            )
        )

    /// The other member in `members`, reaching the library by their own
    /// account.
    private static let accountOther =
        BridgePerson(
            profile: BridgePersonProfile(
                id: "4a1f2c3d-0000-4000-8000-000000000002",
                name: "Ben",
                historySharing: .household,
                avatarVersion: nil
            ),
            membership: BridgeMember(
                id: "4a1f2c3d-0000-4000-8000-000000000002",
                role: .member,
                access: .providerAccount(email: "person-b@example.com"),
                isSelf: false,
                canRemove: true,
                activeDevices: [
                    BridgeActiveDevice(
                        id: 2,
                        name: "Tablet",
                        isSelf: false,
                        canRemove: true
                    )
                ],
                formerDevices: [
                    BridgeFormerDevice(
                        id: 4,
                        name: "Person B's Old Laptop",
                        state: .removed
                    ),
                    BridgeFormerDevice(
                        id: 5,
                        name: "Person B's Tablet",
                        state: .removed
                    ),
                ],
                activeDeviceCount: 1,
                formerDeviceCount: 2
            )
        )

    /// A restore code shaped like coven's: `CVR1-` and uppercase base32,
    /// the one the restore-code board shows.
    static let restoreCode =
        "CVR1-UJZDEGXDNCF32EPF3DHODZDOCIS2JHTLGMXGEDN73U55XTPLPFT"
        + "7V4SEH2KVJ72CEUVW75EFR6EDT4SYWB5WKH7DNSIPZZ7FK4ZRI3R2WYO"
        + "JFLJOOA7LQSAJ2XUID5ZZZZG6ZDMEN4KHVDGAJGXBENYJQWX6HH7566T"
        + "FJGVQ6KBNXJBTFQXKWOVOMPZOM7WBBR6QMW4WXFOGO6MVN6A6WFHYM6L"
        + "3VFZ5ZFKKIBJ5J6WJIBAGI3MNB"

    /// That code in groups of four, as the restore code shows.
    static let restoreCodeGrouped: String = {
        let body = Array(restoreCode.dropFirst("CVR1-".count))
        let groups = stride(from: 0, to: body.count, by: 4)
            .map {
                String(body[$0..<min($0 + 4, body.count)])
            }
        return (["CVR1"] + groups).joined(separator: " ")
    }()

    /// That code as core shares it: grouped, its masked groups, and the
    /// code for the QR.
    static let restoreCodeShare: BridgeRestoreCodeShare = {
        let groups = restoreCodeGrouped.split(separator: " ").map(String.init)
        return BridgeRestoreCodeShare(
            code: restoreCodeGrouped,
            maskedGroups: groups.prefix(2)
                + Array(repeating: "••••", count: groups.count - 3)
                + groups.suffix(1),
            qrText: restoreCode
        )
    }()

    /// A `Sync` whose reads serve shot values, so the library-settings
    /// shots never touch cloud storage.
    static func previewSync() -> Sync {
        Sync(
            followStorageSetup: {
                StorageSetupFollowing(host: BaeHost.inTemporaryFolder())
            },
            generateRestoreCode: { restoreCodeShare },
            addDevice: AddDeviceCalls(start: {
                BridgeAddDevice(
                    id: "6f1f0ab0-0000-4000-8000-000000000001",
                    code: restoreCodeGrouped,
                    qrText: restoreCode
                )
            })
        )
    }

    /// A `ConfigStore` whose library has its storage at `storage`, so
    /// the Library settings shot shows the storage, its sync and the
    /// people.
    @MainActor
    static func connectedConfigStore(
        storage: BridgeStorageLocation
    ) -> ConfigStore {
        makeConfigStore(libraryFullWidth: false, storage: storage)
    }
}
