import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("HistorySharingSection")

/// Who sees what this person plays: only them, or the household. A change
/// applies to new plays; earlier plays keep the audience they were recorded
/// in. A person without a profile yet — a join that couldn't write it —
/// creates one here first.
struct HistorySharingSection: View {
    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync

    @State
    private var name = ""
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        Section("Listening history") {
            if let profile = store.selfProfile {
                Picker(
                    "Share new plays with",
                    selection: Binding(
                        get: { profile.historySharing },
                        set: { sharing in
                            run { try await sync.setHistorySharing(sharing) }
                        }
                    )
                ) {
                    Text("Only me").tag(BridgeSharing.private)
                    Text("Everyone in the library").tag(BridgeSharing.household)
                }
                .disabled(task != nil)
                Text(
                    "Applies to new plays. Earlier plays keep who could see them."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
            }
            else if store.members != nil {
                Text("Create your profile so the library can show who you are.")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                TextField("Your name", text: $name)
                Button("Create Profile") {
                    let name = name.trimmingCharacters(in: .whitespaces)
                    run { _ = try await sync.createProfile(name) }
                }
                .disabled(
                    task != nil
                        || name.trimmingCharacters(in: .whitespaces).isEmpty
                )
            }
            if let error {
                ErrorDetailDisclosure(error: error)
            }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) {
        error = nil
        task = Task {
            defer { task = nil }
            do { try await operation() }
            catch is CancellationError {
                logger.debug("profile change cancelled")
            }
            catch {
                logger.error(
                    "profile change failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
