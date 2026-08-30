import SwiftUI

enum StorageInspectorTab: Hashable {
    case contents
    case transfers
}

/// The trailing inspector for one selected release. Contents and transfer
/// activity share the same release selection and never alter the table shape.
struct StorageInspector: View {
    let releaseId: String
    @Binding
    var isPresented: Bool

    @State
    private var tab: StorageInspectorTab

    init(
        releaseId: String,
        isPresented: Binding<Bool>,
        initialTab: StorageInspectorTab = .contents
    ) {
        self.releaseId = releaseId
        _isPresented = isPresented
        _tab = State(initialValue: initialTab)
    }

    static func releaseId(in selection: Set<String>) -> String? {
        guard selection.count == 1 else { return nil }
        return selection.first
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            Picker("Inspector", selection: $tab) {
                Text("Contents").tag(StorageInspectorTab.contents)
                Text("Transfers").tag(StorageInspectorTab.transfers)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding()

            switch tab {
            case .contents:
                StorageContentsInspector(releaseId: releaseId)
            case .transfers:
                StorageTransferInspector(releaseId: releaseId)
            }
        }
        .frame(
            minWidth: 360,
            idealWidth: 440,
            maxWidth: 520,
            maxHeight: .infinity,
            alignment: .top
        )
    }

    private var header: some View {
        HStack {
            Text("Inspector")
                .font(.headline)
            Spacer()
            Button {
                isPresented = false
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Close")
            .help("Close")
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
    }
}
