import BaeKit
import SwiftUI

struct ImportCombinationReviewView: View {
    @Bindable
    var review: ImportCombinationReview
    let onCancel: () -> Void
    let onCombined: (String) -> Void
    @State
    private var saveTask: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Combine as One Release")
                .font(.title2.weight(.semibold))
            Text(
                "Arrange the selected folders in playback order. Source files stay where they are."
            )
            .foregroundStyle(.secondary)
            TextField("Release name", text: $review.name)
                .textFieldStyle(.roundedBorder)
            Picker(
                "Track numbering",
                selection: Binding(
                    get: { review.order },
                    set: { review.setOrder($0) }
                )
            ) {
                Text("Separate discs")
                    .tag(BridgeCombinationTrackOrder.separateDiscs)
                Text("Continuous numbering")
                    .tag(BridgeCombinationTrackOrder.continuous)
            }
            .pickerStyle(.segmented)
            HStack(alignment: .top, spacing: 24) {
                folders
                    .frame(width: 350)
                Divider()
                tracks
            }
            if let error = review.error {
                ErrorDetailDisclosure(error: error)
            }
            HStack {
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                    .disabled(review.isSaving)
                Spacer()
                if review.isSaving { ProgressView().controlSize(.small) }
                Button("Combine as One Release") {
                    saveTask = Task {
                        if let key = await review.save() { onCombined(key) }
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!review.canSave)
            }
        }
        .padding(28)
        .frame(width: 880, height: 640)
        .disabled(review.isSaving)
        .onDisappear { saveTask?.cancel() }
    }

    private var folders: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Folders").font(.headline)
                ForEach(
                    Array(review.preview.parts.enumerated()),
                    id: \.element.candidateKey
                ) { index, part in
                    HStack(alignment: .top, spacing: 10) {
                        Text((index + 1).formatted())
                            .monospacedDigit().foregroundStyle(.secondary)
                        VStack(alignment: .leading, spacing: 6) {
                            Text(part.folderName).fontWeight(.medium)
                            Text(part.candidateKey)
                                .font(.caption).foregroundStyle(.secondary)
                                .lineLimit(2).truncationMode(.middle)
                                .help(part.candidateKey)
                            Text("\(Int(part.trackCount)) tracks")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                        VStack(spacing: 8) {
                            Button {
                                review.move(index, to: index - 1)
                            } label: {
                                Image(systemName: "chevron.up")
                            }
                            .help("Move Up")
                            .disabled(index == 0)
                            Button {
                                review.move(index, to: index + 1)
                            } label: {
                                Image(systemName: "chevron.down")
                            }
                            .help("Move Down")
                            .disabled(index + 1 == review.keys.count)
                        }
                        .buttonStyle(.borderless)
                    }
                    .padding(12)
                    .background(
                        .primary.opacity(0.04),
                        in: RoundedRectangle(cornerRadius: 8)
                    )
                }
            }
        }
    }

    private var tracks: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text("Tracks").font(.headline)
                ForEach(Array(review.preview.tracks.enumerated()), id: \.offset)
                { _, track in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(
                            coreString("ui.import.sheet.disc", Int(track.side))
                        )
                        .foregroundStyle(.secondary)
                        .frame(width: 56, alignment: .leading)
                        Text(track.trackNumber?.formatted() ?? "—")
                            .monospacedDigit()
                            .frame(width: 28, alignment: .trailing)
                        Text(track.file?.fileId ?? track.title)
                            .font(.system(.caption, design: .monospaced))
                            .lineLimit(2).truncationMode(.middle)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
    }
}
