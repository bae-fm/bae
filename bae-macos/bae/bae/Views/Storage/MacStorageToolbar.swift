import BaeKit
import SwiftUI

/// The Storage Manager window's toolbar: the All / Local / Cloud control,
/// the Filter field and the Inspector button. `off` draws each in its own off
/// state, for the window with no library open, where none of them does
/// anything.
struct MacStorageToolbar: ViewModifier {
    let filter: Binding<BridgeStorageFilter>
    let search: Binding<String>
    let inspectorPresented: Binding<Bool>
    var off = false

    func body(content: Content) -> some View {
        content
            .filterField(search, off: off)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    MacSegmentedControl(
                        selectedIndex: Binding(
                            get: { filter.wrappedValue.segmentIndex },
                            set: {
                                filter.wrappedValue = .init(segmentIndex: $0)
                            }
                        ),
                        segments: [
                            .init(
                                label: String(localized: "All"),
                                systemImage: nil
                            ),
                            .init(
                                label: String(localized: "Local"),
                                systemImage: nil
                            ),
                            .init(
                                label: String(localized: "Cloud"),
                                systemImage: nil
                            ),
                        ],
                        off: off
                    )
                }
                if off {
                    ToolbarItem(placement: .primaryAction) {
                        OffFilterField()
                    }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        inspectorPresented.wrappedValue.toggle()
                    } label: {
                        Image(systemName: "sidebar.trailing")
                            .themeIcon(.small)
                            .frame(
                                width: ThemeSize.hitTarget,
                                height: ThemeSize.hitTarget
                            )
                    }
                    .buttonStyle(
                        IconHoverButtonStyle(
                            isOn: inspectorPresented.wrappedValue
                        )
                    )
                    .accentFocusRing(cornerRadius: ThemeRadius.control)
                    .help("Inspector")
                    .accessibilityLabel(Text("Inspector"))
                    .disabled(off)
                }
            }
    }
}

extension View {
    /// The Filter field in the toolbar. With no library open the field is
    /// off, which the toolbar draws as the words "Filter" (`OffFilterField`)
    /// in place of a field that takes no text.
    @ViewBuilder
    fileprivate func filterField(_ search: Binding<String>, off: Bool)
        -> some View
    {
        if off {
            self
        }
        else {
            searchable(
                text: search,
                placement: .toolbar,
                prompt: Text("Filter")
            )
        }
    }
}

/// The Filter field while it is off: its magnifier and the word Filter on
/// the field's fill, dimmed.
private struct OffFilterField: View {
    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "magnifyingglass")
                .themeIcon(.small)
                .accessibilityHidden(true)
            Text("Filter")
                .themeText(.body)
        }
        .foregroundStyle(.secondary)
        .padding(.horizontal, ThemeSpace.related)
        .frame(width: 160, height: 24, alignment: .leading)
        .background(
            Theme.field,
            in: RoundedRectangle(cornerRadius: ThemeRadius.field)
        )
        .opacity(ThemeOpacity.off)
    }
}

extension BridgeStorageFilter {
    /// The place of this filter among the All / Local / Cloud segments.
    fileprivate var segmentIndex: Int {
        switch self {
        case .all: 0
        case .local: 1
        case .remote: 2
        }
    }

    /// The filter of the segment at `segmentIndex`.
    fileprivate init(segmentIndex: Int) {
        switch segmentIndex {
        case 0: self = .all
        case 1: self = .local
        case 2: self = .remote
        default: preconditionFailure("the control has three segments")
        }
    }
}
