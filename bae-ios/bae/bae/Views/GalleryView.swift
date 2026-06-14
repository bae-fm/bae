import SwiftUI

/// Full-screen artwork viewer over a release's gallery items (cover first, then
/// any synced image files). Swipeable when there's more than one; each item's
/// `localPath` is an absolute path the bridge already resolved.
struct GalleryView: View {
    let items: [BridgeGalleryItem]

    @Environment(\.dismiss)
    private var dismiss
    @State
    private var selection = 0

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            TabView(selection: $selection) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    ImageView(path: item.localPath, pointSize: 1024, contentMode: .fit)
                        .tag(index)
                }
            }
            .tabViewStyle(
                .page(indexDisplayMode: items.count > 1 ? .automatic : .never)
            )
            // The current item's label (e.g. "Cover", "Back.jpg") so multi-image
            // galleries aren't a blind swipe-through. Sits above the page dots
            // and doesn't intercept swipes. `selection` is TabView-bounded and
            // the gallery is never shown empty, so the subscript is safe; core
            // always sets a non-empty label ("Cover" or the filename).
            Text(items[selection].label)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.85))
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .padding(.bottom, 44)
                .allowsHitTesting(false)
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title)
                    .foregroundStyle(.white)
                    .padding()
            }
        }
    }
}
