import BaeKit
import Testing

struct CreditRoleTextTests {
    @Test("a role of one word is that word")
    func plainRole() {
        #expect(BridgeCreditRole.word("Arranger").text == "Arranger")
    }

    @Test("a role's qualifiers follow it in parentheses, as a list")
    func qualifiedRole() {
        let role = BridgeCreditRole(
            role: .verbatim(text: "Engineer"),
            qualifiers: [
                .verbatim(text: "assistant"),
                .verbatim(text: "Strings"),
                .verbatim(text: "Album Design"),
            ]
        )
        #expect(role.text == "Engineer (assistant, Strings, and Album Design)")
    }

    @Test("a role bae names is worded from the catalog")
    func namedRole() {
        let role = BridgeCreditRole(
            role: .localized(key: "core.credit.role.engineer"),
            qualifiers: [.localized(key: "core.credit.qualifier.assistant")]
        )
        #expect(role.text == "Engineer (assistant)")
    }
}
