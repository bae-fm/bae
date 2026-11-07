import SwiftUI
import os.log

private let logger = Logger.bae("ProfileCreationRows")

/// The rows of a person with no profile yet — a join that couldn't ask for
/// it — who creates one here: they say while it is being made, and why it
/// wasn't, when the library refused it or this device stopped syncing for good
/// first. With a profile they show nothing.
public struct ProfileCreationRows: View {
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

    public init() {}

    public var body: some View {
        Group {
            if store.selfProfile != nil {
                EmptyView()
            }
            else if let requested = store.requestedProfile,
                case .making = requested.state
            {
                WaitingIndicator("Making your profile…")
                Text("Your profile appears once this library syncs.")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            else if store.members != nil {
                if let why = store.requestedProfile?.state.unmadeReason {
                    ErrorText("Your profile couldn't be made. \(why)")
                }
                Text("Create your profile so the library can show who you are.")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                TextField("Your name", text: $name)
                Button("Create profile") {
                    let name = name.trimmingCharacters(in: .whitespaces)
                    run { try await sync.createProfile(name) }
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
                    "profile change failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

extension BridgeRequestedProfileState {
    /// Why the profile asked for wasn't made, as a sentence; nil while it is
    /// being made.
    fileprivate var unmadeReason: String? {
        switch self {
        case .making: nil
        case .refused(let reason): reason.label
        case .stopped(let stop): stop.label
        }
    }
}
