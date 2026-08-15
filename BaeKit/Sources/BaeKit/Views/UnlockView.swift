import SwiftUI

public struct UnlockView: View {
    public let libraryName: String
    public let onUnlock: @MainActor (String) async throws -> Void
    /// Back out without unlocking — returns to wherever the unlock was entered
    /// from (the welcome chooser, or the previously-open library on a switch).
    public let onCancel: () -> Void

    public init(
        libraryName: String,
        onUnlock: @escaping @MainActor (String) async throws -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.libraryName = libraryName
        self.onUnlock = onUnlock
        self.onCancel = onCancel
    }

    @State
    private var keyHex: String = ""
    @State
    private var isUnlocking = false
    @State
    private var error: String?

    private var isValidHex: Bool {
        keyHex.count == 64 && keyHex.allSatisfy(\.isHexDigit)
    }

    public var body: some View {
        VStack(spacing: 32) {
            Spacer()
            Image(systemName: "lock.fill")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            VStack(spacing: 8) {
                Text("Library Locked")
                    .font(.title)
                Text(libraryName)
                    .font(.title3)
                    .foregroundStyle(.secondary)
            }
            Text(
                "The encryption key for this library is not in the keyring. Enter the 64-character hex key to unlock."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 400)
            VStack(spacing: 16) {
                SecureField("Encryption key (64 hex characters)", text: $keyHex)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 400)
                    .monospaced()
                HStack(spacing: 12) {
                    Button("Cancel", action: onCancel)
                        .buttonStyle(.bordered)
                        .disabled(isUnlocking)
                    Button(action: unlock) {
                        if isUnlocking {
                            ProgressView()
                                .controlSize(.small)
                        }
                        else {
                            Text("Unlock")
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(!isValidHex || isUnlocking)
                    .keyboardShortcut(.defaultAction)
                }
            }
            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.callout)
            }
            Spacer()
        }
        .padding()
    }

    private func unlock() {
        isUnlocking = true
        error = nil
        Task { @MainActor in
            do {
                try await onUnlock(keyHex)
                isUnlocking = false
            }
            catch {
                isUnlocking = false
                self.error = error.displayLine
            }
        }
    }
}
