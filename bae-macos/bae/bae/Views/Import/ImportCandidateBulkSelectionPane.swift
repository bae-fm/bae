import BaeKit
import SwiftUI

struct ImportCandidateBulkSelectionPane: View {
    @Environment(ImportStore.self)
    private var importStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(ConfigStore.self)
    private var configStore
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    let onPerform: (ImportCandidateActionOffer) -> Void
    @State
    private var confirmation: ImportCandidateActionOffer?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("\(uiStore.selectedFolderCandidates.count) selected")
                    .font(.title2.weight(.semibold))
                Text("Each action applies only to eligible selected folders.")
                    .foregroundStyle(.secondary)
                ForEach(
                    ImportCandidateSelection(
                        importStore: importStore,
                        uiStore: uiStore
                    )
                    .offers
                ) { offer in
                    Button {
                        if offer.action == .useFileMetadata
                            || offer.action == .clearMetadata
                        {
                            confirmation = offer
                        }
                        else {
                            onPerform(offer)
                        }
                    } label: {
                        Label(
                            offer.action.label(count: offer.candidates.count),
                            systemImage: offer.action.symbol
                        )
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(6)
                    }
                    .disabled(uiStore.candidateActionRun.isRunning)
                    if offer.action == .importReady,
                        configStore.config.hasCloudHome
                    {
                        HStack {
                            ImportCheckboxToggle("Cloud", isOn: $storageCloud)
                            if storageCloud {
                                ImportCheckboxToggle(
                                    "Pinned",
                                    isOn: $storagePinned
                                )
                            }
                        }
                        .disabled(uiStore.candidateActionRun.isRunning)
                    }
                }
                if let progress = uiStore.candidateActionRun.progress {
                    ProgressView(
                        value: Double(progress.completed),
                        total: Double(progress.total)
                    ) {
                        Text(progress.action.label(count: progress.total))
                    }
                }
                if uiStore.candidateActionRun.isRunning {
                    Button("Cancel") { uiStore.candidateActionRun.cancel() }
                }
            }
            .frame(maxWidth: 460, alignment: .leading)
            .padding(32)
            .frame(maxWidth: .infinity)
        }
        .alert(
            "Replace selected metadata?",
            isPresented: Binding(
                get: { confirmation != nil },
                set: { if !$0 { confirmation = nil } }
            ),
            presenting: confirmation
        ) { offer in
            Button(
                offer.action.label(count: offer.candidates.count),
                role: .destructive
            ) { onPerform(offer) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text(
                "This replaces metadata and cover choices for the selected folders. Source files and track layout are unchanged."
            )
        }
    }
}
