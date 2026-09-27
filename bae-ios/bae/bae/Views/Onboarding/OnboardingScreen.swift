import BaeKit
import SwiftUI

/// The centered, padded column every onboarding screen sits in.
struct OnboardingScreen<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            content
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(ThemeSpace.page)
    }
}

/// The secondary explanatory line shared by the onboarding screens.
struct OnboardingSecondaryText: View {
    let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .themeText(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 320)
    }
}

#if DEBUG
#Preview {
    // Routed through `String` values so the extractor never takes preview-only
    // prose into the catalog.
    let title = "Screen title"
    let secondary = "A secondary explanatory line."
    OnboardingScreen {
        Text(verbatim: title)
            .themeText(.title)
        OnboardingSecondaryText(LocalizedStringKey(secondary))
    }
}
#endif
