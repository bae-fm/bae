import SwiftUI

// MARK: - Shared sidebar layout

private struct ImportSidebarList<Header: View, Content: View>: View {
    @ViewBuilder
    let header: () -> Header
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                header()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(Theme.surface)
            Divider()
            content()
        }
    }
}

// MARK: - ImportCandidateListContent

struct ImportCandidateListContent: View {
    let candidates: [Candidate]
    @Binding
    var selectedKey: String?
    let isLikelyDupe: (String) -> Bool
    let onAdd: () -> Void
    let onClearAll: () -> Void
    let onClearCompleted: () -> Void
    let onRemove: (String) -> Void

    @State
    private var filterText = ""
    @AppStorage("importCandidateSort")
    private var sortOrder: CandidateSortOrder = .dateAddedNewest

    private var hasImported: Bool {
        candidates.contains { candidate in
            if case .complete = candidate.importStatus {
                return true
            }
            return false
        }
    }

    private var displayedCandidates: [Candidate] {
        var filtered = candidates

        if !filterText.isEmpty {
            let query = filterText.lowercased()
            filtered = filtered.filter {
                $0.displayName.lowercased().contains(query)
                    || $0.key.lowercased().contains(query)
            }
        }

        return switch sortOrder {
        case .nameAZ:
            filtered.sorted {
                $0.displayName.localizedCaseInsensitiveCompare($1.displayName)
                    == .orderedAscending
            }
        case .nameZA:
            filtered.sorted {
                $0.displayName.localizedCaseInsensitiveCompare($1.displayName)
                    == .orderedDescending
            }
        case .dateAddedNewest:
            filtered
        case .dateAddedOldest:
            filtered.reversed()
        }
    }

    var body: some View {
        ImportSidebarList {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.tertiary)
                    .font(.callout)
                TextField("Filter...", text: $filterText)
                    .textFieldStyle(.plain)
                    .font(.callout)
                if !filterText.isEmpty {
                    Button {
                        filterText = ""
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.tertiary)
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: .infinity)
            Menu {
                Section("Sort") {
                    Button {
                        sortOrder = .nameAZ
                    } label: {
                        if sortOrder == .nameAZ {
                            Label("Name (A\u{2013}Z)", systemImage: "checkmark")
                        }
                        else {
                            Text("Name (A\u{2013}Z)")
                        }
                    }
                    Button {
                        sortOrder = .nameZA
                    } label: {
                        if sortOrder == .nameZA {
                            Label("Name (Z\u{2013}A)", systemImage: "checkmark")
                        }
                        else {
                            Text("Name (Z\u{2013}A)")
                        }
                    }
                    Button {
                        sortOrder = .dateAddedNewest
                    } label: {
                        if sortOrder == .dateAddedNewest {
                            Label(
                                "Date Added (Newest)",
                                systemImage: "checkmark"
                            )
                        }
                        else {
                            Text("Date Added (Newest)")
                        }
                    }
                    Button {
                        sortOrder = .dateAddedOldest
                    } label: {
                        if sortOrder == .dateAddedOldest {
                            Label(
                                "Date Added (Oldest)",
                                systemImage: "checkmark"
                            )
                        }
                        else {
                            Text("Date Added (Oldest)")
                        }
                    }
                }
                Divider()
                Button("Clear All", action: onClearAll)
                if hasImported {
                    Button("Clear Imported", action: onClearCompleted)
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            Button(action: onAdd) {
                Image(systemName: "plus")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        } content: {
            List(
                displayedCandidates,
                id: \.key,
                selection: $selectedKey,
            ) { candidate in
                CandidateRow(
                    displayName: candidate.displayName,
                    revealPath: candidate.folderPathIfReal,
                    status: candidate.importStatus,
                    isLikelyDupe: isLikelyDupe(candidate.displayName),
                    onRemove: { onRemove(candidate.key) },
                )
            }
            .scrollContentBackground(.hidden)
            .background(Theme.surface)
        }
    }
}

// MARK: - CandidateRow

struct CandidateRow: View {
    let displayName: String
    /// Real filesystem path for Reveal in Finder. Always set for folder
    /// candidates.
    let revealPath: String?
    let status: ImportStatus?
    let isLikelyDupe: Bool
    let onRemove: () -> Void

    @Environment(OutboxStore.self)
    private var outboxStore

    @State
    private var isHovered = false

    var body: some View {
        HStack(spacing: 8) {
            statusIcon
                .frame(width: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(displayName)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if isLikelyDupe, status == nil {
                    Text("Likely imported")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .opacity(isLikelyDupe ? 0.6 : 1.0)
            Spacer()
            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Remove")
            .opacity(isHovered ? 1 : 0)
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .contextMenu {
            if let revealPath {
                Button("Reveal in Finder") {
                    SystemActions.revealInFinder(path: revealPath)
                }
                Divider()
            }
            Button("Remove") {
                onRemove()
            }
        }
        .onHover { hovering in
            isHovered = hovering
        }
    }

    @ViewBuilder
    private var statusIcon: some View {
        if let status {
            switch status {
            case .importing:
                ProgressView()
                    .controlSize(.small)
            case .complete(_, let releaseId):
                // Imported, but the cloud upload may still be draining — show
                // the transfer until the release's queue empties.
                if let progress = outboxStore.progress(forRelease: releaseId),
                    !progress.isIdle
                {
                    Image(systemName: "icloud.and.arrow.up")
                        .foregroundStyle(.secondary)
                }
                else {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(.green)
                }
            case .error:
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
            }
        }
        else {
            Image(systemName: "folder")
                .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Previews

#Preview("Candidate List") {
    // Inject importStatus into a couple of preview candidates so we can see
    // the badges render.
    let candidates = PreviewData.folderCandidates.map { c -> Candidate in
        var copy = c
        copy.importStatus = PreviewData.importStatuses[c.key]
        return copy
    }
    return ImportCandidateListContent(
        candidates: candidates,
        selectedKey: .constant(candidates[0].key),
        isLikelyDupe: { $0 == "Album Title One" },
        onAdd: {},
        onClearAll: {},
        onClearCompleted: {},
        onRemove: { _ in },
    )
    .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
    .frame(width: 280, height: 500)
}
