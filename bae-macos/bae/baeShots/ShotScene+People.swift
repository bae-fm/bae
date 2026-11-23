import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// People and devices over the Settings window, as the boards draw it:
    /// the first page for an admin and for a member, an invite that is
    /// listed, waiting, joining or refused, a person's page, this person's
    /// devices, the sheet that asks to approve a join request, the sheet
    /// that adds a device and the one that shows it added, the invite form
    /// for each storage, and the invite's sheet sharing, waiting for the
    /// code and showing its code.
    static let peopleScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.people",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(invites: [invited])
        },
        ShotScene(
            id: "mac.settings.people.picture",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(ownPicture: true, invites: [invited])
        },
        ShotScene(
            id: "mac.settings.people.invite-waiting",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(invites: [granting], shown: .fixtureOffline)
        },
        ShotScene(
            id: "mac.settings.people.invite-failed",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(refusedInvites: [
                BridgeRefusedInvite(
                    operation: 7,
                    address: "dan@example.com",
                    addressInitials: "D",
                    refusal: .invalidAddress,
                    reasonKey: "core.invite.refused.invalid_address"
                )
            ])
        },
        ShotScene(
            id: "mac.settings.people.member",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(asAdmin: false)
        },
        ShotScene(
            id: "mac.settings.invite.joining",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(invites: [joining])
        },
        ShotScene(
            id: "mac.settings.person",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(subpage: .person(memberId: otherMemberId))
        },
        ShotScene(
            id: "mac.settings.devices",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(subpage: .devices, ownRemovedDevices: true)
        },
        ShotScene(
            id: "mac.settings.invite.request",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(
                invites: [invitedDan],
                sheet: {
                    AnyView(
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
                }
            )
        },
        ShotScene(
            id: "mac.settings.add-device",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(
                subpage: .devices,
                sheet: {
                    AnyView(
                        AddDeviceSheetContent(
                            result: .success(
                                BridgeAddDevice(
                                    id: "6f1f0ab0-0000-4000-8000-000000000001",
                                    code:
                                        "CVP1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567",
                                    qrText:
                                        "CVP1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
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
                }
            )
        },
        ShotScene(
            id: "mac.settings.add-device.request",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(
                subpage: .devices,
                sheet: {
                    AnyView(
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
                }
            )
        },
        ShotScene(
            id: "mac.settings.add-device.added",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(
                subpage: .devices,
                ownTabletAdded: true,
                sheet: {
                    AnyView(
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
                }
            )
        },
        ShotScene(
            id: "mac.settings.add-device.ended",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(
                subpage: .devices,
                sheet: {
                    AnyView(
                        AddDeviceSheetContent(
                            result: nil,
                            face: .expired,
                            busy: false,
                            error: nil,
                            onApprove: {},
                            onDecline: {},
                            onDismiss: {}
                        )
                    )
                }
            )
        },
        ShotScene(
            id: "mac.settings.invite.dropbox",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteForm(.dropbox)
            })
        },
        ShotScene(
            id: "mac.settings.invite.onedrive",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteForm(.oneDrive)
            })
        },
        ShotScene(
            id: "mac.settings.invite.icloud",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteForm(.iCloud)
            })
        },
        ShotScene(
            id: "mac.settings.invite.s3",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteForm(.s3)
            })
        },
        ShotScene(
            id: "mac.settings.invite.s3-own-key",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteForm(.s3, usesOwnKey: true)
            })
        },
        ShotScene(
            id: "mac.settings.invite.sharing",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteCodeSheet(granting, offline: false)
            })
        },
        ShotScene(
            id: "mac.settings.invite.no-code-yet",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteCodeSheet(granting, offline: true)
            })
        },
        ShotScene(
            id: "mac.settings.invite.code",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(sheet: {
                inviteCodeSheet(invitedDan, offline: false)
            })
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
        ShotScene(
            id: "mac.settings.listening-history.choosing",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            peopleStage(
                menu: {
                    AnyView(
                        MacMenuInPlace(
                            {
                                MacMenu(
                                    BridgeHistorySharing.popUpOptions,
                                    selection: BridgeHistorySharing.private,
                                    select: { _ in }
                                )
                                .entries
                            },
                            highlighted: 1,
                            isPopUp: true
                        )
                        .padding(.leading, 555)
                        .padding(.top, 195)
                    )
                }
            )
        },
    ]

    /// The shot people with the other person named Ben, who has a tablet
    /// and no removed devices, and this person's own removed devices only
    /// when `ownRemovedDevices`: the boards draw them on Your devices alone.
    /// `ownTabletAdded` adds the tablet this person has just added.
    private static func peopleWithBen(
        role: BridgeMemberRole,
        ownPicture: Bool = false,
        ownRemovedDevices: Bool = false,
        ownTabletAdded: Bool = false
    ) -> BridgePeople {
        var people = PreviewData.members(
            reaching: PreviewData.previewDropboxStorage
        )
        if !ownRemovedDevices {
            people = people.ownRemovedDevicesLeftOut()
        }
        if ownTabletAdded {
            people.own?.membership.activeDevices
                .append(
                    BridgeActiveDevice(
                        id: 8,
                        name: "Tablet",
                        isSelf: false,
                        canRemove: true
                    )
                )
        }
        if ownPicture {
            people.own?.profile?.avatarVersion = "1"
        }
        people.others = people.others.map { person in
            guard person.membership.id == otherMemberId else {
                return person
            }
            var ben = person
            ben.profile?.name = "Ben"
            ben.profile?.avatarVersion = "1"
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
            ben.membership.formerDeviceCount = 0
            return ben
        }
        return people
    }

    private static func makeAdmin(
        role: BridgeMemberRole,
        state: BridgeAdminChangeState
    ) -> ShotStage {
        let membership = MembershipStore.stub(
            people: peopleWithBen(role: role),
            selfIsAdmin: true,
            adminChanges: [
                .person(member: otherMemberId): [
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
            ]
        )
        return ShotStage(
            SettingsWindowShot(
                page: PeopleAndDevicesPage(
                    onBack: {},
                    subpage: .person(memberId: otherMemberId)
                )
                .environment(
                    PreviewData.previewSync()
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewDropboxStorage
                    )
                )
                .environment(membership)
            )
        ) {
            membership.people != nil && membership.person.shown != nil
        }
    }

    /// An invite sent to another@example.com, which the person it went to
    /// hasn't answered yet.
    private static let invited = BridgeInvite(
        id: "11",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "another@example.com",
        labelInitials: "A",
        access: .providerAccount(email: "another@example.com"),
        shown: .code(code: "invite-code", progress: .waiting),
        held: nil
    )

    /// An invite sent to dan@example.com, which shows its code and which Dan
    /// hasn't answered yet.
    private static let invitedDan = BridgeInvite(
        id: "11",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "dan@example.com",
        labelInitials: "D",
        access: .providerAccount(email: "dan@example.com"),
        // Shaped as the Add device scene's code, so both QR codes hold 25
        // modules, as the boards draw them.
        shown: .code(
            code: "CVI1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
            progress: .waiting
        ),
        held: nil
    )

    /// An invite sent to dan@example.com whose access sync has not granted.
    private static let granting = BridgeInvite(
        id: "12",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "dan@example.com",
        labelInitials: "D",
        access: .providerAccount(email: "dan@example.com"),
        shown: .granting,
        held: nil
    )

    /// An invite whose person Dan is being let in.
    private static let joining = BridgeInvite(
        id: "13",
        member: "4a1f2c3d-0000-4000-8000-000000000009",
        label: "dan@example.com",
        labelInitials: "D",
        access: .providerAccount(email: "dan@example.com"),
        shown: .code(
            code: "invite-code",
            progress: .joining(name: "Dan", initials: "D")
        ),
        held: nil
    )

    /// The shot people's other person.
    private static let otherMemberId = "4a1f2c3d-0000-4000-8000-000000000002"

    /// The Settings window on the Library pane, showing People and devices
    /// `subpage` with `sheet`, when there is one, laid over it as the boards
    /// draw it.
    private static func peopleStage(
        subpage: PeopleAndDevicesPage.Subpage = .people,
        asAdmin: Bool = true,
        ownPicture: Bool = false,
        ownRemovedDevices: Bool = false,
        ownTabletAdded: Bool = false,
        invites: [BridgeInvite] = [],
        refusedInvites: [BridgeRefusedInvite] = [],
        shown: BridgeSyncShown = .fixtureSynced,
        sheet: (@MainActor () -> AnyView)? = nil,
        menu: (@MainActor () -> AnyView)? = nil
    ) -> ShotStage {
        // A member's board shows Ben as the library's admin.
        var people = peopleWithBen(
            role: asAdmin ? .member : .admin,
            ownPicture: ownPicture,
            ownRemovedDevices: ownRemovedDevices,
            ownTabletAdded: ownTabletAdded
        )
        if !asAdmin {
            people.own?.membership.role = .member
        }
        let membership = MembershipStore.stub(
            people: people,
            selfIsAdmin: asAdmin,
            invites: invites,
            refusedInvites: refusedInvites
        )
        let window = SettingsWindowShot(
            shown: shown,
            page: PeopleAndDevicesPage(onBack: {}, subpage: subpage)
                .environment(
                    PreviewData.previewSync()
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewDropboxStorage
                    )
                )
                .environment(membership)
                .environment(
                    ImageStore(fetchAvatarBytes: {
                        try BoardArtwork.data(named: $0)
                    })
                )
        )
        let framed =
            sheet.map { AnyView(OverWindow.sheet($0(), over: window)) }
            ?? AnyView(window)
        // An open menu sits in the window at the point the board draws it.
        let content =
            menu.map { openMenu in
                AnyView(
                    ZStack(alignment: .topLeading) {
                        framed
                        openMenu()
                    }
                )
            } ?? framed
        return ShotStage(content) {
            if case .person = subpage {
                return membership.people != nil
                    && membership.person.shown != nil
            }
            return membership.people != nil
        }
    }

    /// The invite form for `storage`, filled in for dan@example.com, as the
    /// boards draw it.
    private static func inviteForm(
        _ storage: InviteStorage,
        usesOwnKey: Bool = false
    ) -> AnyView {
        AnyView(
            MacInviteFields(
                storage: storage,
                email: .constant("dan@example.com"),
                accessKeyId: .constant("AKIAEXAMPLEKEY"),
                secretAccessKey: .constant("abcdefghijkl"),
                usesOwnKey: .constant(usesOwnKey),
                role: .constant(.member),
                lifetime: .constant(.sevenDays),
                error: nil,
                canCreate: true,
                onCreate: {},
                onCancel: {}
            )
        )
    }

    /// The sheet of `invite`.
    private static func inviteCodeSheet(
        _ invite: BridgeInvite,
        offline: Bool
    ) -> AnyView {
        AnyView(
            InviteCodeSheetContent(
                invite: invite,
                providerName: "Dropbox",
                offline: offline,
                error: nil,
                busy: false,
                onCancelInvite: {},
                onDone: {}
            )
        )
    }
}

extension BridgePeople {
    /// These people without this person's removed devices, as the boards
    /// draw every devices page but Your devices on its own.
    func ownRemovedDevicesLeftOut() -> BridgePeople {
        var people = self
        people.own?.membership.formerDevices = []
        people.own?.membership.formerDeviceCount = 0
        return people
    }
}
