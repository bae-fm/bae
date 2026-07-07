import Foundation
import Testing

@testable import bae

struct ObjCExceptionGuardTests {
    @Test
    func returnsValueForExistingKey() {
        let object = KeyValueObject()

        let value = ObjCExceptionGuard.value(forKey: "known", on: object)

        #expect(value as? String == "stored")
    }

    @Test
    func returnsNilWhenKeyLookupRaises() {
        let value = ObjCExceptionGuard.value(
            forKey: "missing",
            on: NSObject()
        )

        #expect(value == nil)
    }
}

private final class KeyValueObject: NSObject {
    @objc
    let known = "stored"
}
