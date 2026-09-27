import SwiftUI

public struct UnlockView: View {
    public let libraryName: String
    public let onUnlock: @MainActor (String) async throws -> Void
    /// Back out to wherever the unlock was entered from.
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

    /// The widest the explanation and the key field grow.
    private static let contentWidth: CGFloat = 400

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
        VStack(spacing: ThemeSpace.page) {
            Spacer()
            Image(systemName: "lock.fill")
                .themeIcon(.hero)
                .foregroundStyle(.secondary)
            VStack(spacing: ThemeSpace.line) {
                Text("Library Locked")
                    .themeText(.title)
                Text(libraryName)
                    .themeText(.heading)
                    .foregroundStyle(.secondary)
            }
            Text(
                "The encryption key for this library is not in the keyring. Enter the 64-character hex key to unlock."
            )
            .themeText(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: Self.contentWidth)
            VStack(spacing: ThemeSpace.group) {
                SecureField("Encryption key (64 hex characters)", text: $keyHex)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: Self.contentWidth)
                    .themeText(.mono)
                HStack(spacing: ThemeSpace.group) {
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
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(!isValidHex || isUnlocking)
                    .keyboardShortcut(.defaultAction)
                }
            }
            if let error {
                Text(error)
                    .foregroundStyle(Theme.danger)
                    .themeText(.body)
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
