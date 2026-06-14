import Combine
import SwiftUI

struct CoverSheetView: View {
    let releaseImages: [(id: String, name: String, path: String?)]
    let fetchRemoteCovers: () async throws -> [BridgeRemoteCover]
    let onSelectRemote: (BridgeRemoteCover) -> Void
    let onSelectReleaseImage: (String) -> Void
    let onDone: () -> Void

    @State
    private var remoteCovers: [BridgeRemoteCover] = []
    @State
    private var loading = true
    @State
    private var errorMessage: String?
    @State
    private var refreshSignal = PassthroughSubject<Void, Never>()

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Change Cover")
                    .font(.headline)
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding()
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    if let errorMessage {
                        Text(errorMessage)
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                    Text("Remote Sources")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    if loading {
                        HStack {
                            ProgressView()
                                .controlSize(.small)
                            Text("Fetching covers...")
                                .font(.callout)
                                .foregroundStyle(.secondary)
                        }
                    }
                    else if remoteCovers.isEmpty {
                        Text("No remote covers found")
                            .font(.callout)
                            .foregroundStyle(.tertiary)
                    }
                    else {
                        LazyVGrid(
                            columns: [GridItem(.adaptive(minimum: 120))],
                            spacing: 12
                        ) {
                            ForEach(remoteCovers, id: \.url) { cover in
                                remoteCoverOption(cover)
                            }
                        }
                    }
                    Button(action: { refreshSignal.send() }) {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }
                    .disabled(loading)
                    if !releaseImages.isEmpty {
                        Divider()
                        Text("Release Files")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        LazyVGrid(
                            columns: [GridItem(.adaptive(minimum: 120))],
                            spacing: 12
                        ) {
                            ForEach(
                                Array(releaseImages.enumerated()),
                                id: \.offset
                            ) { _, item in
                                Button(action: { onSelectReleaseImage(item.id) }
                                ) {
                                    VStack(spacing: 4) {
                                        Group {
                                            if let path = item.path {
                                                ImageView(
                                                    source: .local(path: path),
                                                    pointSize: 120
                                                )
                                            }
                                            else {
                                                Theme.placeholder
                                            }
                                        }
                                        .frame(width: 120, height: 120)
                                        .clipShape(
                                            RoundedRectangle(cornerRadius: 6)
                                        )
                                        Text(item.name)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .lineLimit(1)
                                    }
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding()
            }
        }
        .task {
            await loadCovers()
            for await _ in refreshSignal.values {
                await loadCovers()
            }
        }
    }

    @MainActor
    private func loadCovers() async {
        loading = true
        remoteCovers = []
        errorMessage = nil
        do {
            let covers = try await fetchRemoteCovers()
            remoteCovers = covers
            loading = false
        }
        catch is CancellationError {
            // Sheet dismissed mid-fetch; teardown owns the next transition.
        }
        catch {
            errorMessage =
                "Failed to load covers: \(error.localizedDescription)"
            loading = false
        }
    }

    private func remoteCoverOption(_ cover: BridgeRemoteCover) -> some View {
        Button(action: { onSelectRemote(cover) }) {
            VStack(spacing: 4) {
                ImageView(
                    source: .remote(url: cover.thumbnailUrl),
                    pointSize: 120
                )
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(cover.label)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
    }
}

#Preview("Cover Sheet") {
    CoverSheetView(
        releaseImages: [
            (id: "file-1", name: "cover.jpg", path: nil),
            (id: "file-2", name: "back.jpg", path: nil),
        ],
        fetchRemoteCovers: {
            [
                BridgeRemoteCover(
                    url: "https://example.com/cover1.jpg",
                    thumbnailUrl: "https://example.com/thumb1.jpg",
                    label: "Front",
                    source: .musicBrainz
                ),
                BridgeRemoteCover(
                    url: "https://example.com/cover2.jpg",
                    thumbnailUrl: "https://example.com/thumb2.jpg",
                    label: "Back",
                    source: .musicBrainz
                ),
            ]
        },
        onSelectRemote: { _ in },
        onSelectReleaseImage: { _ in },
        onDone: {},
    )
    .frame(width: 500, height: 450)
    .background(Theme.surface)
    .environment(MediaPaths.stub)
}

#Preview("Cover Sheet Loading") {
    CoverSheetView(
        releaseImages: [],
        fetchRemoteCovers: {
            try await Task.sleep(for: .seconds(60))
            return []
        },
        onSelectRemote: { _ in },
        onSelectReleaseImage: { _ in },
        onDone: {},
    )
    .frame(width: 500, height: 450)
    .background(Theme.surface)
    .environment(MediaPaths.stub)
}
