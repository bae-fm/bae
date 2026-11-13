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

    @Test(
        "an instrument on MusicBrainz's list and a voice are worded from the catalog"
    )
    func performedPart() {
        let bassGuitar = BridgeCreditRole(
            role: .localized(
                key:
                    "core.credit.instrument.17f9f065-2312-4a24-8309-6f6dd63e2e33"
            ),
            qualifiers: [.localized(key: "core.credit.qualifier.guest")]
        )
        #expect(bassGuitar.text == "Bass guitar (guest)")
        let lead = BridgeCreditRole(
            role: .localized(
                key: "core.credit.voice.8e2a3255-87c2-4809-a174-98cb3704f1a5"
            ),
            qualifiers: []
        )
        #expect(lead.text == "Lead vocals")
    }
}
