import AuthenticationServices
import SwiftUI
import UIKit

#if BAE_OAUTH_PROVIDERS
/// Captures the host window as the presentation anchor the OAuth web-auth
/// session needs. Rendered as a zero-size background behind the onboarding
/// flow; only the OAuth link branch reads the captured anchor.
struct PresentationAnchorReader: UIViewRepresentable {
    @Binding
    var presentationAnchor: ASPresentationAnchor?

    func makeUIView(context: Context) -> UIView {
        UIView(frame: .zero)
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        let window = uiView.window
        DispatchQueue.main.async {
            presentationAnchor = window
        }
    }
}
#endif
