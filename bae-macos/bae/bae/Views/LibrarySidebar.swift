import Combine
import SwiftUI
import os.log

private let logger = Logger.bae("LibrarySidebar")

/// Left rail listing every library on this device. Each library is a
/// switch-target. The "+" toolbar menu opens the welcome flow (for
/// create / restore).
struct LibrarySidebar: View {
    @Environment(Sync.self)
    private var sync
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(OutboxStore.self)
    private var outboxStore
    let onOpen: (BridgeLibrary) -> Void
    /// Called when a + menu item should present the welcome flow at a
    /// specific mode. `nil` lands on `.choose` (let the user pick).
    let onAddLibrary: (WelcomeView.Mode?) -> Void
    /// Show the library's folder in Finder. The `NSWorkspace` call lives at the
    /// composition root, not this leaf.
    let onRevealInFinder: (BridgeLibrary) -> Void
    /// Put the library id on the pasteboard. The `NSPasteboard` call lives at
    /// the composition root, not this leaf.
    let onCopyLibraryId: (String) -> Void
    /// Fires whenever the library set may have changed (create, restore,
    /// switch); the sidebar refetches the list in place.
    let librariesChanged: AnyPublisher<Void, Never>

    @State
    private var libraries: [BridgeLibrary]?
    @State
    private var loadError: String?
    @State
    private var libraryToLock: BridgeLibrary?
    @State
    private var lockTask: Task<Void, Never>?
    /// Per-library color overrides, JSON-encoded `{id: colorName}`.
    /// Color names match `LibraryColor.allCases`; missing entries fall
    /// back to the default secondary tint.
    @AppStorage("library-colors")
    private var colorsRaw: String = "{}"
    /// Non-nil while the rename sheet is open.
    @State
    private var renameSheet: RenameSheetState?
    @State
    private var renameTask: Task<Void, Never>?
    /// Manual reload triggered by the toolbar Refresh button.
    @State
    private var refreshTask: Task<Void, Never>?
    /// Manual reload triggered when bae becomes the frontmost app.
    @State
    private var activationReloadTask: Task<Void, Never>?
    /// Reload triggered when the library set changes (create/restore/switch).
    @State
    private var openReloadTask: Task<Void, Never>?
    /// User's chosen library ordering, comma-separated ids in display
    /// order. Empty / unset means fall back to the discovery sort (active
    /// first, then by name).
    @AppStorage("library-order")
    private var libraryOrderRaw: String = ""
    /// Library ids the user has chosen to hide from the sidebar,
    /// comma-separated. Per-device preference; the libraries stay on
    /// disk untouched.
    @AppStorage("library-hidden")
    private var hiddenRaw: String = ""
    /// When true, hidden libraries reappear in the list with a
    /// "Hidden" badge so the user can unhide them.
    @AppStorage("show-hidden-libraries")
    private var showHidden: Bool = false

    struct RenameSheetState: Identifiable {
        let id: String
        var newName: String
        var error: String?
    }

    var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                List {
                    Section {
                        // `nil` is not-loaded — render no rows (the header's
                        // `nil` count already signals it). Collapsing it to an
                        // empty array here would hide that distinction.
                        if let visible = visibleLocal {
                            ForEach(visible, id: \.id) {
                                libraryRow($0)
                            }
                            .onMove(perform: moveLocal)
                        }
                    } header: {
                        sectionHeader(count: libraries?.count)
                    }
                    if let loadError {
                        Section {
                            VStack(alignment: .leading, spacing: 6) {
                                Text(loadError)
                                    .foregroundStyle(.red)
                                    .font(.callout)
                                Button("Retry") {
                                    Task { await loadLibraries() }
                                }
                                .buttonStyle(.bordered)
                                .controlSize(.small)
                            }
                        }
                    }
                }
                .onChange(of: activeId) { _, newId in
                    guard let newId else { return }
                    withAnimation { proxy.scrollTo(newId, anchor: .center) }
                }
                .onAppear {
                    if let id = activeId {
                        proxy.scrollTo(id, anchor: .center)
                    }
                }
            }
            statusFooter
        }
        .navigationTitle("Libraries")
        .contextMenu {
            Button("New library...") { onAddLibrary(nil) }
            Button("Restore from code...") { onAddLibrary(.restore) }
        }
        .toolbar {
            ToolbarItem {
                Menu {
                    Button("New library...") { onAddLibrary(nil) }
                        .keyboardShortcut("n", modifiers: [.command, .option])
                    Button("Restore from code...") {
                        onAddLibrary(.restore)
                    }
                    if hasHiddenLibraries {
                        Divider()
                        Toggle("Show Hidden Libraries", isOn: $showHidden)
                    }
                } label: {
                    Label("Add", systemImage: "plus")
                }
            }
            ToolbarItem {
                Button {
                    refreshTask?.cancel()
                    refreshTask = Task { await loadLibraries() }
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .keyboardShortcut("r", modifiers: .command)
                .help("Refresh library list")
            }
        }
        .background(hotSwitchShortcuts)
        .task { await loadLibraries() }
        .onReceive(librariesChanged) { _ in
            openReloadTask?.cancel()
            openReloadTask = Task { await loadLibraries() }
        }
        .onReceive(
            NotificationCenter.default.publisher(
                for: NSApplication.didBecomeActiveNotification
            )
        ) { _ in
            activationReloadTask?.cancel()
            activationReloadTask = Task { await loadLibraries() }
        }
        .onDisappear {
            lockTask?.cancel()
            renameTask?.cancel()
            refreshTask?.cancel()
            activationReloadTask?.cancel()
            openReloadTask?.cancel()
        }
        .sheet(item: $renameSheet) { sheet in
            RenameLibrarySheet(
                state: Binding(
                    get: {
                        renameSheet ?? RenameSheetState(id: "", newName: "")
                    },
                    set: { renameSheet = $0 }
                ),
                onCancel: { renameSheet = nil },
                onCommit: { newName in
                    renameTask?.cancel()
                    renameTask = Task { await doRename(sheet.id, newName) }
                },
            )
        }
        .alert(
            "Lock library?",
            isPresented: Binding(
                get: { libraryToLock != nil },
                set: { if !$0 { libraryToLock = nil } }
            ),
            presenting: libraryToLock
        ) { _ in
            Button("Lock", role: .destructive) {
                lockTask?.cancel()
                lockTask = Task { await doLock() }
            }
            Button("Cancel", role: .cancel) {}
        } message: { lib in
            Text(
                "\(lib.name)'s encryption key will be removed from the keychain. This session keeps working; you'll need to re-enter the key on next launch."
            )
        }
    }

    /// The id of the currently-active local library, if any. Drives the
    /// scroll-to-active behavior when the list opens or the active row
    /// changes (e.g., after a hot-switch shortcut).
    private var activeId: String? {
        libraries?.first(where: \.isActive)?.id
    }

    /// Compact status strip at the bottom of the sidebar: a colored dot
    /// and short label for the active library's sync state. Surfaces
    /// trouble the user might otherwise only notice via the
    /// Library-settings reconnect banner.
    private var statusFooter: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(statusColor)
                .frame(width: 7, height: 7)
            Text(statusLabel)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(Color.gray.opacity(0.08))
        .help(statusHelp)
    }

    private var statusColor: Color {
        if configStore.syncError != nil { return .red }
        if configStore.syncReady { return .green }
        return .gray
    }

    private var statusLabel: String {
        if configStore.syncError != nil { return "Sync error" }
        if configStore.syncReady { return "Synced" }
        return "Sync off"
    }

    private var statusHelp: String {
        configStore.syncError ?? statusLabel
    }

    /// "My Libraries" header with a trailing count. `count` is `nil` until the
    /// library list has loaded; until then nothing renders in the count's place
    /// — not-loaded is distinct from a loaded count of zero.
    private func sectionHeader(count: Int?) -> some View {
        HStack {
            Text("My Libraries")
            Spacer()
            if let count {
                Text("\(count)")
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
            }
        }
    }

    /// Invisible buttons that wire ⌘⇧1 … ⌘⇧9 to switching to the
    /// corresponding row in "My Libraries" (1-indexed). ⌘1–9 are
    /// reserved by MainAppMenuCommands; ⌘⇧ stays out of its way.
    @ViewBuilder
    private var hotSwitchShortcuts: some View {
        if let local = libraries {
            hotSwitchButtons(local)
        }
    }

    @ViewBuilder
    private func hotSwitchButtons(_ local: [BridgeLibrary]) -> some View {
        let keys: [KeyEquivalent] = [
            "1", "2", "3", "4", "5", "6", "7", "8", "9",
        ]
        ForEach(0..<min(local.count, keys.count), id: \.self) { idx in
            Button("") { onOpen(local[idx]) }
                .keyboardShortcut(keys[idx], modifiers: [.command, .shift])
                .opacity(0)
                .allowsHitTesting(false)
        }
        Button("") { switchByOffset(-1) }
            .keyboardShortcut("[", modifiers: [.command, .shift])
            .opacity(0)
            .allowsHitTesting(false)
        Button("") { switchByOffset(1) }
            .keyboardShortcut("]", modifiers: [.command, .shift])
            .opacity(0)
            .allowsHitTesting(false)
    }

    /// Switch to the local library `offset` positions away from the
    /// currently-active one (wrapping around the end). No-op when no
    /// library is active or there's only one local library.
    private func switchByOffset(_ offset: Int) {
        guard let local = libraries,
            local.count > 1,
            let activeIdx = local.firstIndex(where: \.isActive)
        else {
            return
        }
        let count = local.count
        let next = ((activeIdx + offset) % count + count) % count
        onOpen(local[next])
    }

    private func libraryRow(_ lib: BridgeLibrary) -> some View {
        Button {
            onOpen(lib)
        } label: {
            HStack(spacing: 8) {
                LibraryAvatar(library: lib, colorOverride: color(for: lib.id))
                    .opacity(isHidden(lib) ? 0.5 : 1)
                VStack(alignment: .leading, spacing: 1) {
                    Text(lib.name)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .opacity(isHidden(lib) ? 0.5 : 1)
                    // The subtitle reads `lastSyncTime` at the leaf so only it
                    // re-renders when the sync time changes, and it keeps the
                    // line in the layout tree even when empty so the row height
                    // stays stable as libraries load and sync state changes.
                    LibraryRowSubtitle(
                        lib: lib,
                        configStore: configStore
                    )
                }
                // The "Hidden" badge stays in the tree, toggled by opacity so a
                // row's height doesn't change as hidden state flips.
                Text("Hidden")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 1)
                    .background(
                        Capsule().fill(Color.secondary.opacity(0.15))
                    )
                    .opacity(isHidden(lib) ? 1 : 0)
                    .allowsHitTesting(isHidden(lib))
                Spacer()
                // The outbox dot and the sync indicator each read their store at
                // the leaf so only they re-render on outbox/sync change, not the
                // whole row.
                LibraryRowOutboxDot(
                    isActive: lib.isActive,
                    outboxStore: outboxStore
                )
                LibraryRowSyncIndicator(
                    isActive: lib.isActive,
                    configStore: configStore
                )
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(tooltip(for: lib))
        .contextMenu {
            Menu("Color") {
                Button("None") { setColor(nil, for: lib.id) }
                Divider()
                ForEach(LibraryColor.allCases, id: \.self) { c in
                    Button(c.label) { setColor(c, for: lib.id) }
                }
            }
            Button("Rename...") {
                renameSheet = RenameSheetState(
                    id: lib.id,
                    newName: lib.name,
                )
            }
            if lib.isActive {
                Button("Sync Now") { sync.triggerSync() }
                Button("Lock Library...") { libraryToLock = lib }
            }
            else {
                if isHidden(lib) {
                    Button("Unhide") { toggleHidden(lib.id) }
                }
                else {
                    Button("Hide") { toggleHidden(lib.id) }
                }
            }
            Button("Reveal in Finder") { onRevealInFinder(lib) }
            Button("Copy Library ID") { onCopyLibraryId(lib.id) }
        }
    }

    /// Resolved per-library color override, if any.
    private func color(for id: String) -> Color? {
        let map =
            (try? JSONDecoder()
                .decode([String: String].self, from: Data(colorsRaw.utf8)))
            ?? [:]
        guard let name = map[id], let c = LibraryColor(rawValue: name) else {
            return nil
        }
        return c.swiftUIColor
    }

    /// Local libraries to render in the sidebar, or `nil` while the list hasn't
    /// loaded (the section renders no rows). Hidden ones are dropped unless
    /// `showHidden` is on, in which case they appear dimmed with a "Hidden"
    /// badge so the user can unhide them. The active library is never hidden
    /// (would break switching).
    private var visibleLocal: [BridgeLibrary]? {
        guard let ordered = orderedLocal else {
            return nil
        }
        let hidden = hiddenSet
        return ordered.filter {
            $0.isActive || !hidden.contains($0.id) || showHidden
        }
    }

    private var hiddenSet: Set<String> {
        Set(
            hiddenRaw.split(separator: ",")
                .map(String.init)
                .filter { !$0.isEmpty }
        )
    }

    private var hasHiddenLibraries: Bool {
        !hiddenSet.isEmpty
    }

    private func isHidden(_ lib: BridgeLibrary) -> Bool {
        hiddenSet.contains(lib.id)
    }

    private func toggleHidden(_ id: String) {
        var ids = hiddenSet
        if ids.contains(id) {
            ids.remove(id)
        }
        else {
            ids.insert(id)
        }
        hiddenRaw = ids.sorted().joined(separator: ",")
    }

    private func setColor(_ color: LibraryColor?, for id: String) {
        var map =
            (try? JSONDecoder()
                .decode([String: String].self, from: Data(colorsRaw.utf8)))
            ?? [:]
        if let color {
            map[id] = color.rawValue
        }
        else {
            map.removeValue(forKey: id)
        }
        if let data = try? JSONEncoder().encode(map),
            let str = String(data: data, encoding: .utf8)
        {
            colorsRaw = str
        }
    }

    /// Hover tooltip: the full library name plus the filesystem path
    /// that the row truncates / hides for space.
    private func tooltip(for lib: BridgeLibrary) -> String {
        "\(lib.name)\n\(lib.path)"
    }

    private func doLock() async {
        let sync = sync
        do {
            try await DetachedWork.run {
                try sync.lockActiveLibrary()
            }
        }
        catch is CancellationError {
            logger.debug("lock cancelled")
        }
        catch {
            logger.error(
                "Failed to lock library: \(error.localizedDescription)"
            )
            loadError = error.localizedDescription
        }
    }

    /// The local libraries reordered by `libraryOrderRaw`, or `nil` while the
    /// list hasn't loaded yet (distinct from a loaded-but-empty `[]`). Any id in
    /// the stored order that no longer exists is silently dropped; libraries
    /// that appear since the last reorder land at the end in the discovery
    /// sort's original order.
    private var orderedLocal: [BridgeLibrary]? {
        guard let local = libraries else {
            return nil
        }
        if libraryOrderRaw.isEmpty {
            return local
        }
        let storedOrder = libraryOrderRaw.split(separator: ",").map(String.init)
        let byId = Dictionary(uniqueKeysWithValues: local.map { ($0.id, $0) })
        var seen = Set<String>()
        var result: [BridgeLibrary] = []
        for id in storedOrder {
            if let lib = byId[id] {
                result.append(lib)
                seen.insert(id)
            }
        }
        for lib in local where !seen.contains(lib.id) {
            result.append(lib)
        }
        return result
    }

    private func moveLocal(from source: IndexSet, to destination: Int) {
        guard var current = orderedLocal else {
            return
        }
        current.move(fromOffsets: source, toOffset: destination)
        libraryOrderRaw = current.map(\.id).joined(separator: ",")
    }

    private func loadLibraries() async {
        do {
            let result = try await DetachedWork.run {
                try discoverLibraries()
            }
            libraries = result
            loadError = nil
        }
        catch is CancellationError {
            logger.debug("discoverLibraries cancelled")
        }
        catch {
            logger.error(
                "Failed to list libraries: \(error.localizedDescription)"
            )
            loadError = error.localizedDescription
        }
    }

    private func doRename(_ libraryId: String, _ newName: String) async {
        let sync = sync
        let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            try await DetachedWork.run {
                try sync.renameLibrary(libraryId, trimmed)
            }
            renameSheet = nil
            await loadLibraries()
        }
        catch is CancellationError {
            logger.debug("rename cancelled for \(libraryId)")
        }
        catch {
            logger.error(
                "Failed to rename \(libraryId): \(error.localizedDescription)"
            )
            renameSheet?.error = error.localizedDescription
        }
    }
}

/// The per-row subtitle line. The active library shows "Last synced …" relative
/// to the current time once a sync cycle has completed; inactive libraries show
/// their cloud provider label. The line always occupies its slot (a blank space
/// when there's nothing to show), toggled by opacity so the row height stays stable.
///
/// Reads `lastSyncTime` here at the leaf so only this view re-renders when the
/// sync time changes, not the whole row — and reads the clock here too, so the
/// relative label is current each time `lastSyncTime` drives a re-render rather
/// than against a stale snapshot captured by the parent.
private struct LibraryRowSubtitle: View {
    let lib: BridgeLibrary
    let configStore: ConfigStore

    private var text: String? {
        if lib.isActive {
            guard let date = configStore.lastSyncTime else {
                return nil
            }
            let formatter = RelativeDateTimeFormatter()
            formatter.unitsStyle = .abbreviated
            return
                "Last synced \(formatter.localizedString(for: date, relativeTo: Date()))"
        }
        return lib.cloudProviderLabel
    }

    var body: some View {
        let subtitle = text
        Text(subtitle ?? " ")
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(1)
            .truncationMode(.middle)
            .opacity(subtitle == nil ? 0 : 1)
    }
}

/// The trailing pending-outbox dot, shown when the active library has cloud work
/// waiting. The dot stays in the row's layout tree, toggled by opacity so the
/// row height doesn't change as pending state flips.
///
/// Reads `outboxStore.snapshot` here at the leaf so only this view re-renders
/// when the outbox changes, not the whole row.
private struct LibraryRowOutboxDot: View {
    let isActive: Bool
    let outboxStore: OutboxStore

    /// True when the cloud outbox has work waiting — pending uploads/deletes or
    /// items currently uploading.
    private var hasPending: Bool {
        let s = outboxStore.snapshot
        return s.total.pending > 0 || s.pendingDeletes > 0
    }

    private var tooltip: String {
        let s = outboxStore.snapshot
        let total = s.total.pending + s.pendingDeletes
        return
            "\(total) pending sync \(total == 1 ? "operation" : "operations")"
    }

    var body: some View {
        let show = isActive && hasPending
        Circle()
            .fill(Color.orange)
            .frame(width: 6, height: 6)
            .help(tooltip)
            .opacity(show ? 1 : 0)
            .allowsHitTesting(show)
    }
}

/// The trailing sync indicator: a spinner while the active library is mid-sync,
/// otherwise a checkmark on the active library. Both stay in the tree, toggled
/// by opacity so the trailing control's footprint is constant across states.
///
/// Reads `configStore.syncing` here at the leaf so only this view re-renders
/// when the sync state changes, not the whole row.
private struct LibraryRowSyncIndicator: View {
    let isActive: Bool
    let configStore: ConfigStore

    var body: some View {
        let isSyncing = isActive && configStore.syncing
        ZStack {
            ProgressView()
                .controlSize(.mini)
                .opacity(isSyncing ? 1 : 0)
                .allowsHitTesting(isSyncing)
            Image(systemName: "checkmark")
                .font(.caption)
                .foregroundStyle(.secondary)
                .opacity(isActive && !isSyncing ? 1 : 0)
        }
    }
}
