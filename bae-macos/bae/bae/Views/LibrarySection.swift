import SwiftUI

struct LibrarySection: View {
    var body: some View {
        LibraryView()
            .toolbar(.hidden)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
