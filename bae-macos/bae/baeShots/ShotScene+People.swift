import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// People and devices: the first page for an admin and for a member, a
    /// person's page, this person's devices, the sheet that renames this
    /// person, and the sheet that asks to approve a join request.
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
                    title: "Dan",
                    confirmation: "412 778",
                    busy: false,
                    error: nil,
                    onApprove: {},
                    onDecline: {},
                    onClose: {}
                )
            )
        },
    ]

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
