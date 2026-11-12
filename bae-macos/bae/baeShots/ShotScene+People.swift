import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// People and devices: the first page for an admin and for a member, a
    /// person's page, this person's devices, the sheet that renames this
    /// person, the sheet that asks to approve a join request, and the sheet that
    /// adds a device, with its code and once a device has joined.
    static let peopleScenes: [ShotScene] = [
        people("people-and-devices-admin", asAdmin: true, invites: [invited]),
        people(
            "people-and-devices-invite-failed",
            asAdmin: true,
            refusedInvites: [
                BridgeRefusedInvite(
                    operation: 7,
                    address: "dan@example.com",
                    refusal: .invalidAddress
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
                            share: BridgeRestoreCodeShare(
                                code:
                                    "CVR1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567",
                                qrText: "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
                                qrCarriesS3Key: false
                            ),
                            watch: BridgeAddedDeviceWatch(
                                noHandle: BridgeAddedDeviceWatch.NoHandle()
                            )
                        )
                    ),
                    addedDevice: nil,
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
                    addedDevice: BridgeActiveDevice(
                        id: 7,
                        name: "Tablet",
                        isSelf: false,
                        canRemove: true
                    ),
                    onDismiss: {}
                )
            )
        },
    ]

    /// A person's page while the change that makes them an admin waits for
    /// sync, and once it is done: the change's box above their role, which
    /// reads Member until the library has the change.
    static let adminChangeScenes: [ShotScene] = [
        makeAdmin(
            "mac.settings.person.make-admin-waiting",
            role: .member,
            state: .waiting
        ),
        makeAdmin(
            "mac.settings.person.make-admin-done",
            role: .admin,
            state: .done
        ),
    ]

    /// The preview people with the other person named Ben, who has a tablet.
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
        _ id: String,
        role: BridgeMemberRole,
        state: BridgeAdminChangeState
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 500, height: 480)) {
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
                PeopleAndDevicesPage(
                    onBack: {},
                    subpage: .person(memberId: otherMemberId)
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
            ) {
                membership.members != nil
            }
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

    /// The preview people's other person.
    private static let otherMemberId = "4a1f2c3d-0000-4000-8000-000000000002"

    private static func people(
        _ id: String,
        asAdmin: Bool,
        subpage: PeopleAndDevicesPage.Subpage = .people,
        renaming: Bool = false,
        invites: [BridgeInvite] = [],
        refusedInvites: [BridgeRefusedInvite] = []
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
            ) {
                membership.members != nil
            }
        }
    }
}
