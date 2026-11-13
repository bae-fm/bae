import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// People and devices: the first page for an admin and for a member, a
    /// person's page, this person's devices, the sheet that renames this
    /// person, the sheet that asks to approve a join request, the sheet that
    /// adds a device, with its code and once a device has joined, the
    /// invite form for each storage, and the invite's sheet sharing, offline
    /// and showing its code.
    static let peopleScenes: [ShotScene] = [
        people("people-and-devices-admin", asAdmin: true, invites: [invited]),
        people(
            "people-and-devices-invite-offline",
            asAdmin: true,
            invites: [granting],
            offline: true
        ),
        people(
            "people-and-devices-invite-joining",
            asAdmin: true,
            invites: [joining]
        ),
        people(
            "people-and-devices-invite-failed",
            asAdmin: true,
            refusedInvites: [
                BridgeRefusedInvite(
                    operation: 7,
                    address: "dan@example.com",
                    refusal: .invalidAddress,
                    reasonKey: "core.invite.refused.invalid_address"
                )
            ]
        ),
        people("people-and-devices-member", asAdmin: false),
        people(
            "people-and-devices-person",
            asAdmin: true,
            subpage: .person(memberId: otherMemberId)
        ),
        people("people-and-devices-yours", asAdmin: true, subpage: .devices),
        people("people-and-devices-rename", asAdmin: true, renaming: true),
        ShotScene(
            id: "people-and-devices-request",
            size: CGSize(width: 400, height: 190)
        ) {
            ShotStage(
                JoinRequestSheet(
                    name: "Dan",
                    confirmation: "412 778",
                    busy: false,
                    error: nil,
                    onApprove: {},
                    onDecline: {},
                    onClose: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-add-device",
            size: CGSize(width: 440, height: 360)
        ) {
            ShotStage(
                AddDeviceSheetContent(
                    result: .success(
                        BridgeAddDevice(
                            id: "6f1f0ab0-0000-4000-8000-000000000001",
                            code:
                                "CVP1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567",
                            qrText: "CVP1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
                        )
                    ),
                    face: .code,
                    busy: false,
                    error: nil,
                    onApprove: {},
                    onDecline: {},
                    onDismiss: {}
                )
            )
        },
        ShotScene(
            id: "mac.settings.add-device.request",
            size: CGSize(width: 440, height: 220)
        ) {
            ShotStage(
                AddDeviceSheetContent(
                    result: nil,
                    face: .request(
                        deviceName: "Tablet",
                        confirmation: "412 778"
                    ),
                    busy: false,
                    error: nil,
                    onApprove: {},
                    onDecline: {},
                    onDismiss: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-add-device-added",
            size: CGSize(width: 440, height: 130)
        ) {
            ShotStage(
                AddDeviceSheetContent(
                    result: nil,
                    face: .added(name: "Tablet"),
                    busy: false,
                    error: nil,
                    onApprove: {},
                    onDecline: {},
                    onDismiss: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-dropbox",
            size: CGSize(width: 440, height: 300)
        ) {
            ShotStage(
                InvitePersonForm(
                    storage: .dropbox,
                    email: .constant(""),
                    accessKeyId: .constant(""),
                    secretAccessKey: .constant(""),
                    usesOwnKey: .constant(false),
                    role: .constant(.member),
                    lifetime: .constant(.sevenDays),
                    error: nil,
                    canCreate: false,
                    onCreate: {},
                    onCancel: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-onedrive",
            size: CGSize(width: 440, height: 300)
        ) {
            ShotStage(
                InvitePersonForm(
                    storage: .oneDrive,
                    email: .constant(""),
                    accessKeyId: .constant(""),
                    secretAccessKey: .constant(""),
                    usesOwnKey: .constant(false),
                    role: .constant(.member),
                    lifetime: .constant(.sevenDays),
                    error: nil,
                    canCreate: false,
                    onCreate: {},
                    onCancel: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-icloud",
            size: CGSize(width: 440, height: 300)
        ) {
            ShotStage(
                InvitePersonForm(
                    storage: .iCloud,
                    email: .constant(""),
                    accessKeyId: .constant(""),
                    secretAccessKey: .constant(""),
                    usesOwnKey: .constant(false),
                    role: .constant(.member),
                    lifetime: .constant(.sevenDays),
                    error: nil,
                    canCreate: false,
                    onCreate: {},
                    onCancel: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-s3",
            size: CGSize(width: 440, height: 330)
        ) {
            ShotStage(
                InvitePersonForm(
                    storage: .s3,
                    email: .constant(""),
                    accessKeyId: .constant(""),
                    secretAccessKey: .constant(""),
                    usesOwnKey: .constant(false),
                    role: .constant(.member),
                    lifetime: .constant(.sevenDays),
                    error: nil,
                    canCreate: false,
                    onCreate: {},
                    onCancel: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-s3-own-key",
            size: CGSize(width: 440, height: 260)
        ) {
            ShotStage(
                InvitePersonForm(
                    storage: .s3,
                    email: .constant(""),
                    accessKeyId: .constant(""),
                    secretAccessKey: .constant(""),
                    usesOwnKey: .constant(true),
                    role: .constant(.member),
                    lifetime: .constant(.sevenDays),
                    error: nil,
                    canCreate: false,
                    onCreate: {},
                    onCancel: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-sharing",
            size: CGSize(width: 440, height: 190)
        ) {
            ShotStage(
                InviteCodeSheetContent(
                    invite: BridgeInvite(
                        id: "11",
                        member: "4a1f2c3d-0000-4000-8000-000000000009",
                        label: "dan@example.com",
                        access: .providerAccount(email: "dan@example.com"),
                        shown: .granting,
                        held: nil
                    ),
                    providerName: "Dropbox",
                    offline: false,
                    error: nil,
                    busy: false,
                    onCancelInvite: {},
                    onDone: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-offline",
            size: CGSize(width: 440, height: 190)
        ) {
            ShotStage(
                InviteCodeSheetContent(
                    invite: BridgeInvite(
                        id: "11",
                        member: "4a1f2c3d-0000-4000-8000-000000000009",
                        label: "dan@example.com",
                        access: .providerAccount(email: "dan@example.com"),
                        shown: .granting,
                        held: nil
                    ),
                    providerName: "Dropbox",
                    offline: true,
                    error: nil,
                    busy: false,
                    onCancelInvite: {},
                    onDone: {}
                )
            )
        },
        ShotScene(
            id: "people-and-devices-invite-code",
            size: CGSize(width: 440, height: 400)
        ) {
            ShotStage(
                InviteCodeSheetContent(
                    invite: BridgeInvite(
                        id: "11",
                        member: "4a1f2c3d-0000-4000-8000-000000000009",
                        label: "dan@example.com",
                        access: .providerAccount(email: "dan@example.com"),
                        shown: .code(code: "invite-code", progress: .waiting),
                        held: nil
                    ),
                    providerName: "Dropbox",
                    offline: false,
                    error: nil,
                    busy: false,
                    onCancelInvite: {},
                    onDone: {}
                )
            )
        },
    ]

    /// A person's page while the change that makes them an admin waits for
    /// sync, and once it is done: the change's box above their role, which
    /// reads Member until the library has the change.
    static let adminChangeScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.person.make-admin-waiting",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            makeAdmin(role: .member, state: .waiting)
        },
        ShotScene(
            id: "mac.settings.person.make-admin-done",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            makeAdmin(role: .admin, state: .done)
        },
    ]

    /// The shot people with the other person named Ben, who has a tablet.
    private static func peopleWithBen(role: BridgeMemberRole) -> [BridgePerson]
    {
        PreviewData.members(reaching: PreviewData.previewStorage)
            .map {
                person in
                guard person.membership.id == otherMemberId else {
                    return person
                }
                var ben = person
                ben.profile?.name = "Ben"
                ben.membership.role = role
                ben.membership.activeDevices = [
                    BridgeActiveDevice(
                        id: 5,
                        name: "Tablet",
                        isSelf: false,
                        canRemove: true
                    )
                ]
                ben.membership.formerDevices = []
                return ben
            }
    }

    private static func makeAdmin(
        role: BridgeMemberRole,
        state: BridgeAdminChangeState
    ) -> ShotStage {
        let membership = MembershipStore.stub(
            people: peopleWithBen(role: role),
            selfIsAdmin: true,
            adminChanges: [
                BridgeAdminChange(
                    operation: 1,
                    change: .setRole(
                        member: otherMemberId,
                        person: "Ben",
                        role: .admin
                    ),
                    state: state
                )
            ]
        )
        return ShotStage(
            SettingsWindowShot(
                page: PeopleAndDevicesPage(
                    onBack: {},
                    subpage: .person(memberId: otherMemberId)
                )
                .environment(
                    PreviewData.previewSync(storage: PreviewData.previewStorage)
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewStorage
                    )
                )
                .environment(membership)
            )
        ) {
            membership.members != nil
        }
    }

    /// An invite sent to another@example.com, which hasn't been answered.
    private static let invited = BridgeInvite(
        id: "11",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "another@example.com",
        access: .providerAccount(email: "another@example.com"),
        shown: .code(code: "invite-code", progress: .waiting),
        held: nil
    )

    /// An invite sent to dan@example.com whose access sync has not granted.
    private static let granting = BridgeInvite(
        id: "12",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "dan@example.com",
        access: .providerAccount(email: "dan@example.com"),
        shown: .granting,
        held: nil
    )

    /// An invite whose person Dan is being let in.
    private static let joining = BridgeInvite(
        id: "13",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "dan@example.com",
        access: .providerAccount(email: "dan@example.com"),
        shown: .code(code: "invite-code", progress: .joining(name: "Dan")),
        held: nil
    )

    /// The shot people's other person.
    private static let otherMemberId = "4a1f2c3d-0000-4000-8000-000000000002"

    private static func people(
        _ id: String,
        asAdmin: Bool,
        subpage: PeopleAndDevicesPage.Subpage = .people,
        renaming: Bool = false,
        invites: [BridgeInvite] = [],
        refusedInvites: [BridgeRefusedInvite] = [],
        offline: Bool = false
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 480)) {
            let membership = MembershipStore.stub(
                people: PreviewData.members(
                    reaching: PreviewData.previewStorage
                ),
                selfIsAdmin: asAdmin,
                invites: invites,
                refusedInvites: refusedInvites
            )
            return ShotStage(
                PeopleAndDevicesPage(
                    onBack: {},
                    subpage: subpage,
                    renaming: renaming
                )
                .frame(width: 500, height: 480)
                .environment(
                    PreviewData.previewSync(storage: PreviewData.previewStorage)
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewStorage
                    )
                )
                .environment(membership)
                .environment(syncStatus(offline: offline))
            ) {
                membership.members != nil
            }
        }
    }

    /// The sync state the page reads: offline, or not read at all.
    private static func syncStatus(offline: Bool) -> SyncStatusStore {
        guard offline else { return SyncStatusStore() }
        return SyncStatusStore(
            snapshot: BridgeSyncStatusSnapshot(
                shown: .fixtureOffline,
                pending: [],
                setupRunning: false,
                undoneSetup: nil
            )
        )
    }
}
