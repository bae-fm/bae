import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("PeopleAndDevicesPage")

/// The provider step a removed device needs. Its own person, who did the
/// step, tells the library it is done, when their account holds the
/// library's storage.
private struct CutOffStep {
    let device: UInt64
    let instruction: BridgeProviderSignOut
    let offersCutOff: Bool
}

enum CutOffReason {
    case providerRequestRunning
    case deviceNotGone
}

/// A removed device whose cut-off confirmation changed nothing, and why.
private struct CutOffRetry {
    let device: UInt64
    let reason: CutOffReason
}

/// The alerts of cutting a removed device off from the provider.
enum CutOffAlerts {
    /// The step still to do in the provider, `instruction`. When this
    /// person's account holds the library's storage (`offersCutOff`), they say
    /// they did it; otherwise OK.
    static func step(
        _ instruction: BridgeProviderSignOut,
        offersCutOff: Bool,
        onCutOff: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        let words = MacAlertWords(
            Text("bae will remove the device"),
            message: Text(verbatim: instruction.text)
        )
        guard offersCutOff else {
            return (words, [MacAlertButton(Text("OK"))])
        }
        return (
            words,
            [
                MacAlertButton(Text("I cut off this device"), action: onCutOff),
                MacAlertButton(Text("Not yet"), role: .cancel),
            ]
        )
    }

    /// Why confirming the cut-off changed nothing, with Try again.
    static func retry(
        _ reason: CutOffReason,
        onRetry: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        let title: Text
        let message: Text
        let dismiss: Text
        switch reason {
        case .providerRequestRunning:
            title = Text("This device is still syncing with the cloud")
            message = Text("Wait for the sync to end.")
            dismiss = Text("Cancel")
        case .deviceNotGone:
            title = Text("This device’s removal has not reached this Mac yet")
            message = Text("Try again after the next sync.")
            dismiss = Text("Not yet")
        }
        return (
            MacAlertWords(title, message: message),
            [
                MacAlertButton(Text("Try again"), action: onRetry),
                MacAlertButton(dismiss, role: .cancel),
            ]
        )
    }
}

/// The library's people and devices, on pages of their own. The first page has
/// You (name, role, listening history, devices) and People, one row for each
/// other person, with the invites this device has open; an admin's person row
/// opens that person's page (their role, removing them, their devices), and a
/// member's opens nothing. Your devices lists this person's devices, with Add
/// device… under the box. Admins invite people, change roles and remove
/// people; each person adds and removes their own devices, and an admin can
/// remove anyone's. An invite's request opens a sheet to approve or decline
/// it.
struct PeopleAndDevicesPage: View {
    /// Back to the Library page.
    let onBack: () -> Void

    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync
    @Environment(SyncStatusStore.self)
    private var syncStatus

    enum Subpage: Equatable {
        case people
        case person(memberId: String)
        case devices
    }

    /// The invite sheet showing: the form, or an open invite's code.
    private enum InviteSheet: Identifiable {
        case form
        case code(inviteId: String)

        var id: String {
            switch self {
            case .form: ""
            case .code(let inviteId): inviteId
            }
        }
    }

    @State
    private var subpage: Subpage
    @State
    private var renaming = false
    @State
    private var inviteSheet: InviteSheet?
    /// Invites whose request sheet the person closed; the invite's row
    /// opens it again.
    @State
    private var closedRequests: Set<String> = []
    @State
    private var showAddDevice = false
    @State
    private var removal: Removal?
    /// What the person still has to do in the provider after removing a
    /// device.
    @State
    private var signOut: CutOffStep?
    /// The removed device whose cut-off confirmation changed nothing, and why:
    /// confirming again works once it ends or after the next sync.
    @State
    private var cutOffRetry: CutOffRetry?
    @State
    private var task: Task<Void, Never>?
    @State
    private var actionError: DisplayError?

    private enum Removal {
        case person(BridgeMember, name: String)
        case device(BridgeActiveDevice)
    }

    init(
        onBack: @escaping () -> Void,
        subpage: Subpage = .people
    ) {
        self.onBack = onBack
        _subpage = State(initialValue: subpage)
    }

    var body: some View {
        VStack(spacing: 0) {
            MacPageHeading(
                title: Text(verbatim: title),
                back: .init(title: backTitle, action: back)
            )
            Form {
                if let error = store.error {
                    MacSection { MacNotice(error: error) }
                }
                if let actionError {
                    MacSection { MacNotice(error: actionError) }
                }
                switch shown {
                case .people:
                    AdminChangesSection(place: .people)
                    youSection
                    peopleSection
                case .person(let memberId):
                    if let person = person(memberId) {
                        personSections(person)
                    }
                case .devices:
                    if let me = selfPerson {
                        devicesSections(me)
                    }
                }
            }
            .formStyle(.settings)
        }
        .onChange(of: subpage, initial: true) { old, page in
            if case .person(let memberId) = page {
                store.showPerson(memberId)
            }
            else {
                store.hidePerson()
            }
            if old != page, let place = Self.adminChangePlace(old) {
                store.hideAdminChanges(at: place)
            }
            if let place = Self.adminChangePlace(page) {
                store.showAdminChanges(at: place)
            }
        }
        .onChange(of: closedRequests, initial: true) { _, closed in
            store.showWaitingRequest(closed: closed)
        }
        .onDisappear {
            store.hidePerson()
            store.hideWaitingRequest()
            if let place = Self.adminChangePlace(subpage) {
                store.hideAdminChanges(at: place)
            }
        }
        .sheet(isPresented: $renaming) {
            RenameProfileSheet(
                name: store.selfProfile?.name ?? "",
                onDismiss: { renaming = false }
            )
        }
        .sheet(item: $inviteSheet) { sheet in
            switch sheet {
            case .form:
                InvitePersonSheet(
                    onCreated: { inviteSheet = .code(inviteId: $0) },
                    onDismiss: { inviteSheet = nil }
                )
            case .code(let inviteId):
                InviteCodeSheet(
                    inviteId: inviteId,
                    onDismiss: { inviteSheet = nil }
                )
            }
        }
        .sheet(
            isPresented: Binding(
                get: { requestShown != nil },
                set: { shown in
                    if !shown, let invite = requestShown {
                        closedRequests.insert(invite.id)
                    }
                }
            )
        ) {
            if let invite = requestShown,
                let request = invite.requestToDecide
            {
                JoinRequestSheet(
                    name: request.info().name,
                    confirmation: request.info().confirmation,
                    busy: task != nil,
                    error: actionError,
                    onApprove: { approve(invite) },
                    onDecline: { decline(invite) },
                    onClose: { closedRequests.insert(invite.id) }
                )
            }
        }
        .sheet(isPresented: $showAddDevice) {
            AddDevicePairingSheet(onDismiss: { showAddDevice = false })
        }
        .macAlert(item: $removal) { selected in
            let title: String
            let message: Text
            switch selected {
            case .person(let member, let name):
                title = String(localized: "Remove \(name)?")
                let loses = Text(
                    "They lose access to the library. Their private history and private playlists are deleted; what they shared stays."
                )
                message =
                    if let forgetLine = member.access?.forgetLine {
                        loses + Text(verbatim: "\n\n\(forgetLine)")
                    }
                    else {
                        loses
                    }
            case .device(let device):
                title = String(localized: "Remove \(device.name)?")
                message = Text("The device loses access to the library.")
            }
            return (
                MacAlertWords(Text(verbatim: title), message: message),
                [
                    MacAlertButton(Text("Remove"), role: .destructive) {
                        remove(selected)
                    },
                    MacAlertButton(Text("Cancel"), role: .cancel),
                ]
            )
        }
        .macAlert(item: $signOut) { step in
            CutOffAlerts.step(
                step.instruction,
                offersCutOff: step.offersCutOff
            ) { confirmCutOff(step.device) }
        }
        .macAlert(item: $cutOffRetry) { retry in
            CutOffAlerts.retry(retry.reason) { confirmCutOff(retry.device) }
        }
    }

    // MARK: - Pages

    /// The page to show: a person's page whose person has left the library
    /// falls back to the first page.
    private var shown: Subpage {
        if case .person(let memberId) = subpage,
            let read = store.person.shown, read.key == memberId,
            read.value == nil
        {
            return .people
        }
        return subpage
    }

    private var title: String {
        switch shown {
        case .people:
            String(localized: "People and devices")
        case .person(let memberId):
            person(memberId).map(displayName)
                ?? String(localized: "This person")
        case .devices:
            String(localized: "Your devices")
        }
    }

    private var backTitle: String {
        switch shown {
        case .people: String(localized: "Library")
        case .person, .devices: String(localized: "People and devices")
        }
    }

    private func back() {
        switch shown {
        case .people: onBack()
        case .person, .devices: subpage = .people
        }
    }

    /// Returns the person whose page is open when the store's shown person
    /// has the member id this page holds.
    private func person(_ memberId: String) -> BridgePerson? {
        guard let shown = store.person.shown, shown.key == memberId
        else { return nil }
        return shown.value
    }

    private var selfPerson: BridgePerson? {
        store.people?.own
    }

    private func displayName(_ person: BridgePerson) -> String {
        person.profile?.name ?? String(localized: "No profile yet")
    }
}

// MARK: - The pages' sections

extension PeopleAndDevicesPage {
    // MARK: - You

    private var youSection: some View {
        MacSection {
            if let profile = store.selfProfile {
                PictureRow(profile: profile, busy: task != nil, run: run)
                MacRow("Name") {
                    HStack(spacing: ThemeSpace.related) {
                        Text(verbatim: profile.name)
                            .foregroundStyle(.secondary)
                        Button("Change…") { renaming = true }
                    }
                }
            }
            if let me = selfPerson {
                MacRow("Role", value: me.membership.role.label)
            }
            if let profile = store.selfProfile {
                MacRow("Listening history") {
                    MacPopUpButton(
                        "Listening history",
                        selection: Binding(
                            get: { profile.historySharing },
                            set: { sharing in
                                run {
                                    try await sync.setHistorySharing(sharing)
                                }
                            }
                        )
                    ) {
                        BridgeHistorySharing.popUpOptions
                    }
                    .disabled(task != nil)
                }
            }
            if let me = selfPerson {
                PageLinkRow(
                    title: "Devices",
                    value: deviceCount(me.membership),
                    action: { subpage = .devices }
                )
            }
        } header: {
            Text("You")
        } footer: {
            if store.selfProfile != nil {
                Text(
                    "Who sees what you play, or Off to keep no record. Applies to new plays; earlier plays keep who could see them."
                )
            }
        }
    }

    // MARK: - People

    private var peopleSection: some View {
        MacSection {
            ForEach(others, id: \.membership.id) { person in
                let member = person.membership
                // Only an admin has anything to do on a person's page.
                PageLinkRow(
                    title: displayName(person),
                    avatar: person.profile.map { Avatar(profile: $0, size: 28) }
                        ?? Avatar(initials: "", size: 28),
                    tag: member.role.label,
                    value: deviceCount(member),
                    opens: store.selfIsAdmin,
                    action: { subpage = .person(memberId: member.id) }
                )
            }
            ForEach(store.inviteEntries, id: \.rowId) { entry in
                switch entry {
                case .open(let invite):
                    InviteRow(
                        invite: invite,
                        offline: syncStatus.offline,
                        onOpen: { open(invite) }
                    )
                case .refused(let refused):
                    RefusedInviteRow(
                        refused: refused,
                        onDismiss: {
                            run {
                                try await sync.dismissRefusedInvite(
                                    refused.operation
                                )
                            }
                        }
                    )
                }
            }
        } header: {
            Text("People")
        } footer: {
            // The add action sits under the box, on the right, as System
            // Settings puts "Add Account…".
            if store.selfIsAdmin {
                MacSheetFooter {
                    Button("Invite…") { inviteSheet = .form }
                }
            }
        }
    }

    private var others: [BridgePerson] {
        store.people?.others ?? []
    }

    private func deviceCount(_ member: BridgeMember) -> String {
        // An Int makes the key "%lld devices", as the string catalog has it.
        String(
            localized: "\(Int(member.activeDeviceCount)) devices",
            comment: "How many devices a person has in the library"
        )
    }

    // MARK: - A person

    @ViewBuilder
    private func personSections(_ person: BridgePerson) -> some View {
        let member = person.membership
        AdminChangesSection(place: .person(member: String(member.id)))
        MacSection {
            MacRow("Role") {
                HStack(spacing: ThemeSpace.related) {
                    Text(verbatim: member.role.label)
                        .foregroundStyle(.secondary)
                    if store.selfIsAdmin && !member.isSelf {
                        switch member.role {
                        case .admin:
                            Button("Make member") { setRole(member, .member) }
                        case .member:
                            Button("Make admin") { setRole(member, .admin) }
                        }
                    }
                }
            }
            .disabled(task != nil)
            // Every member of a library reaches it the same way, so every
            // person has this line, or none does.
            if let keyLine = member.access?.keyLine {
                Text(verbatim: keyLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            if member.canRemove {
                MacRow("Remove from library…") {}
                    .pressing(
                        .removes {
                            removal = .person(member, name: displayName(person))
                        }
                    )
                    .disabled(task != nil)
            }
        }
        devicesSections(person)
    }

    // MARK: - Devices

    /// A person's devices, each with a ⋯ menu that removes it when allowed; on
    /// this person's own page the device this Mac is reads "This device" with
    /// no menu, and Add device… sits under the box.
    @ViewBuilder
    private func devicesSections(_ person: BridgePerson) -> some View {
        let member = person.membership
        MacSection {
            ForEach(member.activeDevices, id: \.id) { device in
                MacRow {
                    if device.canRemove {
                        MacMoreMenu {
                            MacMenuItem("Remove…") {
                                removal = .device(device)
                            }
                        }
                        .disabled(task != nil)
                    }
                    else if device.isSelf {
                        Text("This device")
                            .foregroundStyle(.secondary)
                    }
                } label: {
                    Text(verbatim: device.name)
                }
            }
        } header: {
            // This person's own devices are the page; another's are a
            // section of theirs.
            if !member.isSelf {
                Text("Devices")
            }
        } footer: {
            if member.isSelf {
                MacSheetFooter {
                    Button("Add device…") { showAddDevice = true }
                }
            }
        }
        if member.formerDeviceCount > 0 {
            FormerDevicesDisclosure(
                devices: member.formerDevices,
                count: member.formerDeviceCount
            )
        }
    }

    /// The place whose admin changes `page` shows; the devices page shows
    /// none.
    private static func adminChangePlace(_ page: Subpage)
        -> BridgeAdminChangePlace?
    {
        switch page {
        case .people: .people
        case .person(let memberId): .person(member: memberId)
        case .devices: nil
        }
    }

    // MARK: - Actions

    /// The first invite whose request waits for a decision, other than
    /// those whose sheet the person closed, as the query last returned it.
    /// When the person closes a sheet, the page passes its invite to the
    /// query, and the sheet stays until the query returns without it.
    private var requestShown: BridgeInvite? {
        store.waitingRequest.shown?.value
    }

    /// Open an invite from its row: its request to decide, or its code.
    private func open(_ invite: BridgeInvite) {
        if invite.requestToDecide != nil {
            closedRequests.remove(invite.id)
        }
        else {
            inviteSheet = .code(inviteId: invite.id)
        }
    }

    private func approve(_ invite: BridgeInvite) {
        guard let request = invite.requestToDecide else { return }
        run { try await sync.approveJoinRequest(request) }
    }

    private func decline(_ invite: BridgeInvite) {
        guard let request = invite.requestToDecide else { return }
        run { try await sync.declineJoinRequest(request) }
    }

    private func setRole(_ member: BridgeMember, _ role: BridgeMemberRole) {
        run { try await sync.setMemberRole(member.id, role) }
    }

    private func remove(_ removal: Removal) {
        switch removal {
        case .person(let member, _):
            run { try await sync.removeMember(member.id) }
            // The person is gone: their page with them.
            subpage = .people
        case .device(let device):
            run {
                let removed = try await sync.removeDevice(device.id)
                signOut = removed.instruction.map {
                    CutOffStep(
                        device: device.id,
                        instruction: $0,
                        offersCutOff: removed.offersCutOff
                    )
                }
            }
        }
    }

    /// Tell the library the person cut a removed device off at the provider.
    private func confirmCutOff(_ device: UInt64) {
        run {
            switch try await sync.confirmDeviceCutOff(device) {
            case .confirmed: break
            case .providerRequestRunning:
                cutOffRetry = CutOffRetry(
                    device: device,
                    reason: .providerRequestRunning
                )
            case .deviceNotGone:
                cutOffRetry = CutOffRetry(
                    device: device,
                    reason: .deviceNotGone
                )
            }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) {
        actionError = nil
        task = Task {
            defer { task = nil }
            do { try await operation() }
            catch is CancellationError {
                logger.debug("membership change cancelled")
            }
            catch {
                logger.error(
                    "membership change failed: \(String(reflecting: error))"
                )
                actionError = DisplayError(error)
            }
        }
    }
}

/// A row that names a page, or a person: a title, what it holds, and a
/// chevron when it opens something.
private struct PageLinkRow: View {
    let title: LocalizedStringKey?
    let verbatimTitle: String?
    /// The person's picture or initials before the title, when the row is a
    /// person's.
    var avatar: Avatar?
    var tag: String?
    let value: String
    var opens = true
    let action: () -> Void

    init(
        title: LocalizedStringKey,
        value: String,
        action: @escaping () -> Void
    ) {
        self.title = title
        verbatimTitle = nil
        self.value = value
        self.action = action
    }

    init(
        title: String,
        avatar: Avatar,
        tag: String,
        value: String,
        opens: Bool,
        action: @escaping () -> Void
    ) {
        self.title = nil
        verbatimTitle = title
        self.avatar = avatar
        self.tag = tag
        self.value = value
        self.opens = opens
        self.action = action
    }

    var body: some View {
        MacRow {
            Text(verbatim: value)
                .foregroundStyle(.secondary)
        } label: {
            if let avatar {
                avatar
            }
            if let title {
                Text(title)
            }
            else if let verbatimTitle {
                Text(verbatim: verbatimTitle)
            }
        }
        .chip(tag)
        .strong(verbatimTitle != nil)
        .pressing(opens ? .opens(action) : nil)
    }
}

/// Renames this device's person's profile, which everyone in the library
/// sees. A failure stays in the sheet with the name as typed.
private struct RenameProfileSheet: View {
    let currentName: String
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var name: String
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    init(name: String, onDismiss: @escaping () -> Void) {
        self.currentName = name
        self.onDismiss = onDismiss
        _name = State(initialValue: name)
    }

    private var trimmed: String {
        name.trimmingCharacters(in: .whitespaces)
    }

    var body: some View {
        MacSheet(verbatim: currentName, width: 420) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                FieldColumn {
                    MacTextField(
                        "Name",
                        note: "Everyone in the library sees this name."
                    ) {
                        TextField("Name", text: $name)
                            .labelsHidden()
                            .boxedTextField()
                    }
                }
                .disabled(task != nil)
                if let error {
                    MacNotice(
                        title: String(localized: "Could not save the name."),
                        advice: nil,
                        error: error
                    )
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel", action: onDismiss)
                    .keyboardShortcut(.cancelAction)
                MacButton("Save", kind: .primary, action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(task != nil || trimmed.isEmpty)
            }
        }
        .onDisappear { task?.cancel() }
    }

    private func save() {
        error = nil
        let name = trimmed
        task = Task {
            defer { task = nil }
            do {
                try await sync.renameProfile(name)
                onDismiss()
            }
            catch is CancellationError {
                logger.debug("profile rename cancelled")
            }
            catch {
                logger.error(
                    "profile rename failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// An invite a sharing refusal ended, in its row's place: the address it was
/// to share with, why the provider refused and what to fix, and Dismiss,
/// which removes the notice for good.
private struct RefusedInviteRow: View {
    let refused: BridgeRefusedInvite
    let onDismiss: () -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(verbatim: refused.address)
                    .themeText(.rowTitle)
                MacErrorLine(
                    String(
                        localized:
                            "Could not invite: \(localizedCoreString(refused.reasonKey))"
                    )
                )
            }
            Spacer()
            MacButton("Dismiss", kind: .borderless, action: onDismiss)
        }
        .padding(.vertical, ThemeSpace.line)
    }
}

/// One of this device's open invites: the address it shares with (the key's
/// id on S3), marked invited, which a tap opens; "Shares access once online"
/// while sync can't grant it because this Mac is offline; and why coven holds
/// its work, when it does. Once approval has begun the row is the person's
/// name, marked joining, with the address beside it, and opens nothing:
/// nothing is left to decide or cancel.
private struct InviteRow: View {
    let invite: BridgeInvite
    let offline: Bool
    let onOpen: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            if case .code(_, .joining(let name, let initials)) = invite.shown {
                PageLinkRow(
                    title: name,
                    avatar: Avatar(initials: initials, size: 28),
                    tag: String(localized: "Joining"),
                    value: invite.label,
                    opens: false,
                    action: {}
                )
            }
            else {
                PageLinkRow(
                    title: invite.label,
                    avatar: Avatar(initials: invite.labelInitials, size: 28),
                    tag: String(localized: "Invited"),
                    value: waitsForSharing
                        ? String(localized: "Shares access once online") : "",
                    opens: true,
                    action: onOpen
                )
                if let held = invite.held {
                    MacErrorLine(localizedCoreString(held.reason.key))
                }
            }
        }
        .padding(.vertical, ThemeSpace.line)
    }

    /// Sync can't share the library with the address while this Mac is
    /// offline, and nothing else holds it.
    private var waitsForSharing: Bool {
        guard case .granting = invite.shown else { return false }
        return offline && invite.held == nil
    }
}

/// A join request, titled with the name of who asks, and the number their
/// device shows, which Approve is for when it matches. Closing it keeps the request; the
/// invite's row opens it again.
struct JoinRequestSheet: View {
    let name: String
    let confirmation: String
    let busy: Bool
    let error: DisplayError?
    let onApprove: () -> Void
    let onDecline: () -> Void
    let onClose: () -> Void

    var body: some View {
        MacSheet(
            "\(name) wants to join",
            message: "Approve them if their screen shows this code:",
            width: 400
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                MacMatchCode(code: confirmation, dialog: true)
                if let error {
                    MacNotice(error: error)
                }
            }
        } footer: {
            MacSheetFooter(
                lead: {
                    Button("Close", action: onClose)
                        .keyboardShortcut(.cancelAction)
                },
                trailing: {
                    Button("Decline", action: onDecline)
                        .disabled(busy)
                    MacButton("Approve", kind: .primary, action: onApprove)
                        .disabled(busy)
                }
            )
        }
    }
}
