import SwiftUI
import os.log

private let logger = Logger.bae("AbandonedAttachmentsView")

/// The uploads this device abandoned because their rows took a newer change
/// first, one row each: what the file was, why it never uploaded, and
/// Dismiss, which takes it off the list.
public struct AbandonedAttachmentsView: View {
    public let attachments: BridgeAbandonedAttachments

    public init(attachments: BridgeAbandonedAttachments) {
        self.attachments = attachments
    }

    public var body: some View {
        switch attachments {
        case .loading:
            EmptyView()
        case .failed(let error):
            ErrorDetailDisclosure(error: DisplayError(error))
        case .ready(let attachments):
            ForEach(attachments, id: \.id) { attachment in
                AbandonedAttachmentRow(attachment: attachment)
            }
        }
    }
}

private struct AbandonedAttachmentRow: View {
    let attachment: BridgeAbandonedAttachment

    @Environment(SyncActivityStore.self)
    private var store
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(attachment.subject.localizedLine)
                .themeText(.strong)
            Text(localizedCoreString(attachment.reasonKey))
                .foregroundStyle(.secondary)
            Button(localizedCoreString("core.abandoned.action.dismiss")) {
                dismiss()
            }
            .disabled(task != nil)
            ErrorDetailDisclosure(error: error)
                .opacity(error == nil ? 0 : 1)
                .accessibilityHidden(error == nil)
        }
        .onDisappear { task?.cancel() }
    }

    private func dismiss() {
        error = nil
        task = Task {
            defer { task = nil }
            do { try await store.dismiss(attachment) }
            catch is CancellationError {
                logger.debug("dismissing cancelled as its row closed")
            }
            catch {
                logger.error("Dismissing an abandoned upload failed: \(error)")
                self.error = DisplayError(error)
            }
        }
    }
}
