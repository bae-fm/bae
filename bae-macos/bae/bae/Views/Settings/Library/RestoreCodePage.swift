import AppKit
import BaeKit
import SwiftUI

/// The restore code on one page of paper: its QR code and the code in groups
/// of four, where the library is stored and the date, and how to use it and
/// keep it. Never the library's name, and never the S3 key: the QR code holds
/// the code to type, which has none.
struct RestoreCodePage: View {
    /// The code in groups of four.
    let code: String
    let location: String
    let date: Date

    static let size = CGSize(width: 612, height: 792)

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("bae restore code")
                .font(.system(size: 22, weight: .semibold))
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: location)
                Text(
                    "Printed \(date.formatted(date: .long, time: .omitted))",
                    comment:
                        "On the printed restore code, the day it was printed"
                )
            }
            .foregroundStyle(Color(white: 0.27))
            QRCode(text: Self.scannedText(code))
                .frame(maxWidth: .infinity)
            Text(verbatim: code)
                .font(.system(size: 12, design: .monospaced))
                .lineSpacing(6)
            Text(
                "If you lose all your devices, open bae on a new one and choose Scan code to scan this, or Enter code to type the code. Anyone who has this page and access to your cloud can open your library: keep it somewhere private."
            )
            .foregroundStyle(Color(white: 0.27))
            Spacer(minLength: 0)
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(white: 0.07))
        .padding(.horizontal, 64)
        .padding(.vertical, 56)
        .frame(width: Self.size.width, height: Self.size.height)
        .background(Color.white)
        .environment(\.colorScheme, .light)
    }

    /// What the QR code holds: the code run together, which decodes as typed
    /// does.
    static func scannedText(_ code: String) -> String {
        code.filter { !$0.isWhitespace }
    }
}

/// Prints a [RestoreCodePage] through the system's print panel.
@MainActor
enum RestoreCodePrinting {
    static func print(code: String, location: String) {
        let page = RestoreCodePage(code: code, location: location, date: .now)
        let view = NSHostingView(rootView: page)
        view.frame = NSRect(origin: .zero, size: RestoreCodePage.size)
        let info = NSPrintInfo.shared.copy() as? NSPrintInfo ?? NSPrintInfo()
        info.topMargin = 0
        info.bottomMargin = 0
        info.leftMargin = 0
        info.rightMargin = 0
        info.isHorizontallyCentered = true
        info.isVerticallyCentered = false
        let operation = NSPrintOperation(view: view, printInfo: info)
        operation.jobTitle = String(
            localized: "bae restore code",
            comment: "The print job's name for the restore code"
        )
        operation.run()
    }
}
