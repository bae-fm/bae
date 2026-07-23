import SwiftUI

/// The centered, padded column every onboarding screen sits in: content
/// vertically centered between two spacers, filling the safe area.
struct OnboardingScreen<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            content
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(32)
    }
}

/// The secondary explanatory line shared by the onboarding screens: centered,
/// width-capped, secondary color.
struct OnboardingSecondaryText: View {
    let text: LocalizedStringKey

    init(_ text: LocalizedStringKey) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 320)
    }
}
