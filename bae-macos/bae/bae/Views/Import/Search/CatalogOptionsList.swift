import BaeKit
import SwiftUI

/// The catalog numbers a folder carries, as rows. Picking one tells the run
/// which to look up; at most one is chosen, so picking another replaces it.
///
/// A folder can carry thirty of them, which is why they are a list rather than
/// a chip each.
struct CatalogOptionsList: View {
    let options: [BridgeSignalOption]
    let onChoose: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            ForEach(options, id: \.value) { option in
                Button {
                    onChoose(option.value)
                } label: {
                    HStack(spacing: 7) {
                        SignalCheckbox(isOn: option.chosen)
                        Text(option.value)
                            .font(.system(size: 11.5, design: .monospaced))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer(minLength: 8)
                        Text(SignalBadgeStyle.originLabel(for: option.origin))
                            .font(.system(size: 10.5))
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                    }
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// A checkbox drawn in SwiftUI rather than taken from `Toggle`.
///
/// AppKit renders a checkbox's label as its button title, which flattens a
/// custom row to text — the value, the count and the origin all disappear. A
/// row that draws its own box keeps its layout.
struct SignalCheckbox: View {
    @Environment(\.accentChoice)
    private var accent
    let isOn: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: 3.5)
            .fill(isOn ? accent.buttonColor : .clear)
            .overlay {
                RoundedRectangle(cornerRadius: 3.5)
                    .strokeBorder(
                        isOn ? .clear : Color.primary.opacity(0.25),
                        lineWidth: 1.5
                    )
            }
            .overlay {
                Image(systemName: "checkmark")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(.white)
                    .opacity(isOn ? 1 : 0)
            }
            .frame(width: 13, height: 13)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Catalog options") {
        CatalogOptionsList(
            options: PreviewData.toolbarCatalogChoices.signals[1].options,
            onChoose: { _ in },
        )
        .padding()
        .frame(width: 300)
        .windowBackground()
    }
#endif
