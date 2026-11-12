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

private enum CutOffReason {
    case providerRequestRunning
    case deviceNotGone
}

/// A removed device whose cut-off confirmation changed nothing, and why.
private struct CutOffRetry {
    let device: UInt64
    let reason: CutOffReason
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
    /// The QR code and restore code the Add Device sheet shows, with the
    /// watch for the device that joins from them; nil while they generate.
    @State
    private var addDeviceShare: Result<BridgeAddDevice, Error>?
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
        /// A device, and whether it is this person's own.
        case device(BridgeActiveDevice, own: Bool)
    }

    init(
        onBack: @escaping () -> Void,
        subpage: Subpage = .people,
        renaming: Bool = false
    ) {
        self.onBack = onBack
        _subpage = State(initialValue: subpage)
        _renaming = State(initialValue: renaming)
    }

    var body: some View {
        VStack(spacing: 0) {
            SettingsPageHeader(
                title: title,
                backTitle: backTitle,
                onBack: back
            )
            Form {
                if let error = store.error {
                    Section { Notice(error: error) }
                }
                if let actionError {
                    Section { Notice(error: actionError) }
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
            .formStyle(.grouped)
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
                    title: request.info().name,
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
            AddDeviceSheet(
                result: $addDeviceShare,
                onDismiss: { showAddDevice = false }
            )
            .task { await generateAddDeviceShare() }
        }
        .confirmationDialog(
            removalTitle,
            isPresented: Binding(
                get: { removal != nil },
                set: { if !$0 { removal = nil } }
            ),
            titleVisibility: .visible,
            presenting: removal
        ) { selected in
            Button("Remove", role: .destructive) { remove(selected) }
            Button("Cancel", role: .cancel) {}
        } message: { selected in
            switch selected {
            case .person(let member, _):
                if let forgetLine = member.access.forgetLine {
                    Text(
                        "They lose access to the library. Their private history and private playlists are deleted; what they shared stays."
                    ) + Text(verbatim: "\n\n\(forgetLine)")
                }
                else {
                    Text(
                        "They lose access to the library. Their private history and private playlists are deleted; what they shared stays."
                    )
                }
            case .device:
                Text("The device loses access to the library.")
            }
        }
        .alert(
            "The device will be removed",
            isPresented: Binding(
                get: { signOut != nil },
                set: { if !$0 { signOut = nil } }
            ),
            presenting: signOut
        ) { step in
            if step.offersCutOff {
                Button("I cut off this device") {
                    confirmCutOff(step.device)
                }
                Button("Not yet", role: .cancel) {}
            }
            else {
                Button("OK", role: .cancel) {}
            }
        } message: { step in
            Text(step.instruction.text)
        }
        .alert(
            cutOffRetry?.reason == .deviceNotGone
                ? "This device’s removal has not reached this Mac yet"
                : "This device is still syncing with the cloud",
            isPresented: Binding(
                get: { cutOffRetry != nil },
                set: { if !$0 { cutOffRetry = nil } }
            ),
            presenting: cutOffRetry
        ) { retry in
            Button("Try again") { confirmCutOff(retry.device) }
            Button("Not yet", role: .cancel) {}
        } message: { retry in
            switch retry.reason {
            case .providerRequestRunning:
                Text("Wait for it to end, then try again.")
            case .deviceNotGone:
                Text("Try again after the next sync.")
            }
        }
    }

    // MARK: - Pages

    /// The page to show: a person's page whose person has left the library
    /// falls back to the first page.
    private var shown: Subpage {
        if case .person(let memberId) = subpage, person(memberId) == nil {
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

    private func person(_ memberId: String) -> BridgePerson? {
        store.members?.first { $0.membership.id == memberId }
    }

    private var selfPerson: BridgePerson? {
        store.members?.first { $0.membership.isSelf }
    }

    private func displayName(_ person: BridgePerson) -> String {
        person.profile?.name ?? String(localized: "No profile yet")
    }
}

// MARK: - The pages' sections

extension PeopleAndDevicesPage {
    // MARK: - You

    private var youSection: some View {
        Section {
            if let profile = store.selfProfile {
                LabeledContent("Name") {
                    HStack(spacing: ThemeSpace.related) {
                        Text(verbatim: profile.name)
                            .foregroundStyle(.secondary)
                        Button("Change…") { renaming = true }
                    }
                }
            }
            else {
                ProfileCreationRows()
            }
            if let me = selfPerson {
                LabeledContent("Role", value: me.membership.role.label)
            }
            if let profile = store.selfProfile {
                Picker(
                    "Listening history",
                    selection: Binding(
                        get: { profile.historySharing },
                        set: { sharing in
                            run { try await sync.setHistorySharing(sharing) }
                        }
                    )
                ) {
                    Text("Off").tag(BridgeHistorySharing.off)
                    Text("Only me").tag(BridgeHistorySharing.private)
                    Text("Everyone").tag(BridgeHistorySharing.household)
                }
                .disabled(task != nil)
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
        Section {
            ForEach(others, id: \.membership.id) { person in
                let member = person.membership
                // Only an admin has anything to do on a person's page.
                PageLinkRow(
                    title: displayName(person),
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
                        onOpen: { open(invite) },
                        onRefresh: {
                            run {
                                actionError = try await sync.nudges.retryNow()
                                    .failure
                            }
                        }
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
        (store.members ?? []).filter { !$0.membership.isSelf }
    }

    private func deviceCount(_ member: BridgeMember) -> String {
        String(
            localized: "\(member.activeDevices.count) devices",
            comment: "How many devices a person has in the library"
        )
    }

    // MARK: - A person

    @ViewBuilder
    private func personSections(_ person: BridgePerson) -> some View {
        let member = person.membership
        AdminChangesSection(place: .person(member: String(member.id)))
        Section {
            LabeledContent("Role") {
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
            if let keyLine = member.access.keyLine {
                Text(verbatim: keyLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            if member.canRemove {
                Button("Remove from library…", role: .destructive) {
                    removal = .person(member, name: displayName(person))
                }
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
        Section {
            ForEach(member.activeDevices, id: \.id) { device in
                LabeledContent {
                    if device.canRemove {
                        MacMoreMenu {
                            Button("Remove…") {
                                removal = .device(device, own: member.isSelf)
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
                    Button("Add device…") {
                        addDeviceShare = nil
                        showAddDevice = true
                    }
                }
            }
        }
        if !member.formerDevices.isEmpty {
            FormerDevicesDisclosure(devices: member.formerDevices)
        }
    }

    // MARK: - Actions

    /// The first invite whose request waits for a decision, unless the
    /// person closed its sheet.
    private var requestShown: BridgeInvite? {
        store.invites.first {
            $0.requestToDecide != nil && !closedRequests.contains($0.id)
        }
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

    private var removalTitle: String {
        switch removal {
        case .person(_, let name): String(localized: "Remove \(name)?")
        case .device(let device, _): String(localized: "Remove \(device.name)?")
        case nil: ""
        }
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
        case .device(let device, let own):
            run {
                let instruction = try await sync.removeDevice(device.id)
                let offersCutOff = own ? await holdsStore() : false
                signOut = instruction.map {
                    CutOffStep(
                        device: device.id,
                        instruction: $0,
                        offersCutOff: offersCutOff
                    )
                }
            }
        }
    }

    /// Whether this person's account holds the library's storage. When the
    /// read fails nothing is offered, since confirming would only be refused.
    private func holdsStore() async -> Bool {
        do {
            return try await sync.holdsStore()
        }
        catch {
            logger.error(
                "Couldn't read whether this account holds the storage: \(String(reflecting: error))"
            )
            return false
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

    /// Closing the sheet cancels this task, and with it the Rust call.
    private func generateAddDeviceShare() async {
        do {
            addDeviceShare = .success(try await sync.addDevice())
        }
        catch is CancellationError {
            logger.debug("add-device code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate the add-device code: \(String(reflecting: error))"
            )
            // No line means core reported a cancellation; keep the spinner.
            guard DisplayError(error) != nil else { return }
            addDeviceShare = .failure(error)
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
        tag: String,
        value: String,
        opens: Bool,
        action: @escaping () -> Void
    ) {
        self.title = nil
        verbatimTitle = title
        self.tag = tag
        self.value = value
        self.opens = opens
        self.action = action
    }

    var body: some View {
        if opens {
            Button(action: action) { row }
                .buttonStyle(.plain)
        }
        else {
            row
        }
    }

    private var row: some View {
        HStack(spacing: ThemeSpace.related) {
            if let title {
                Text(title)
            }
            else if let verbatimTitle {
                Text(verbatim: verbatimTitle)
                    .themeText(.rowTitle)
            }
            if let tag {
                StatusChip(verbatim: tag)
            }
            Spacer()
            Text(verbatim: value)
                .foregroundStyle(.secondary)
            if opens {
                Image(systemName: "chevron.forward")
                    .themeIcon(.small)
                    .foregroundStyle(.tertiary)
            }
        }
        .contentShape(Rectangle())
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
                    Notice(
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
                Button("Save", action: save)
                    .buttonStyle(PrimaryButtonStyle())
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

/// A person's removed and replaced devices, collapsed until opened. They no
/// longer have access, so they offer no actions.
private struct FormerDevicesDisclosure: View {
    let devices: [BridgeFormerDevice]

    @State
    private var isExpanded = false

    var body: some View {
        FormDisclosure(isExpanded: $isExpanded) {
            ForEach(devices, id: \.id) { device in
                HStack(spacing: ThemeSpace.related) {
                    Text(device.name)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                    Text(device.state.label)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
            }
        } label: {
            Text("Removed devices (\(devices.count))")
                .themeText(.detail)
                .foregroundStyle(.secondary)
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
                ErrorText(
                    String(
                        localized:
                            "Could not invite: \(localizedCoreString(bridgeSharingRefusalKey(refusal: refused.refusal)))"
                    )
                )
            }
            Spacer()
            Button("Dismiss", action: onDismiss)
                .buttonStyle(.borderless)
        }
        .padding(.vertical, ThemeSpace.line)
    }
}

/// One of this device's open invites: the address it shares with (the key's
/// id on S3), marked sharing
/// access until sync has granted it, then invited, or joining once approval
/// has begun; why coven holds its work, when it does; and a refresh that checks for its request now.
private struct InviteRow: View {
    let invite: BridgeInvite
    let onOpen: () -> Void
    let onRefresh: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            HStack(spacing: ThemeSpace.related) {
                Button(action: onOpen) {
                    HStack(spacing: ThemeSpace.related) {
                        Text(verbatim: invite.label)
                            .themeText(.rowTitle)
                        switch invite.shown {
                        case .granting:
                            StatusChip("Sharing access", tone: .activity)
                        case .code(_, .waiting), .code(_, .deciding):
                            StatusChip("Invited")
                        case .code(_, .joining):
                            StatusChip("Joining", tone: .activity)
                        }
                        Spacer()
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Button(action: onRefresh) {
                    Image(systemName: "arrow.clockwise")
                        .themeIcon(.small)
                }
                .buttonStyle(.borderless)
                .help("Check for requests")
            }
            if let held = invite.held {
                ErrorText(localizedCoreString(held.reason.key))
            }
        }
        .padding(.vertical, ThemeSpace.line)
    }
}

/// A join request: the person's name, and the number their device shows,
/// which Approve is for when it matches. Closing it keeps the request; the
/// invite's row opens it again.
struct JoinRequestSheet: View {
    let title: String
    let confirmation: String
    let busy: Bool
    let error: DisplayError?
    let onApprove: () -> Void
    let onDecline: () -> Void
    let onClose: () -> Void

    var body: some View {
        MacSheet(
            verbatim: title,
            message: "Make sure this code is showing:",
            width: 400
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                MacMatchCode(code: confirmation, dialog: true)
                if let error {
                    Notice(error: error)
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
                    Button("Approve", action: onApprove)
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(busy)
                }
            )
        }
    }
}

#if DEBUG
    #Preview("People and devices") {
        PeopleAndDevicesPage(onBack: {})
            .frame(width: 500, height: 480)
            .environment(
                PreviewData.previewSync(storage: PreviewData.previewStorage)
            )
            .environment(
                PreviewData.connectedConfigStore(
                    storage: PreviewData.previewStorage
                )
            )
            .environment(
                MembershipStore.stub(
                    people: PreviewData.members(
                        reaching: PreviewData.previewStorage
                    ),
                    selfIsAdmin: true
                )
            )
    }
#endif
