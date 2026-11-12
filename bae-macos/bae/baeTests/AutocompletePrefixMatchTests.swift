import Foundation
import Testing

@testable import bae

/// The autocomplete's prefix match ignores case by the locale's rules.
@Suite("Autocomplete prefix match")
struct AutocompletePrefixMatchTests {
    @Test("case differences do not matter")
    func ignoresCase() {
        let english = Locale(identifier: "en")
        #expect("Björk".hasPrefixIgnoringCase("bjö", locale: english))
        #expect("ÉMILE".hasPrefixIgnoringCase("ém", locale: english))
        #expect(!"Björk".hasPrefixIgnoringCase("bjork", locale: english))
    }

    @Test("an empty prefix matches everything")
    func emptyPrefix() {
        #expect("abc".hasPrefixIgnoringCase(""))
        #expect("".hasPrefixIgnoringCase(""))
    }

    @Test("Turkish pairs I with dotless ı and İ with i")
    func turkishDottedI() {
        let turkish = Locale(identifier: "tr")
        let english = Locale(identifier: "en")
        #expect("ISTANBUL".hasPrefixIgnoringCase("ıst", locale: turkish))
        #expect("İstanbul".hasPrefixIgnoringCase("ist", locale: turkish))
        #expect(!"ISTANBUL".hasPrefixIgnoringCase("ıst", locale: english))
        #expect(!"İstanbul".hasPrefixIgnoringCase("ist", locale: english))
    }
}
