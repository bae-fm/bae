import Foundation

extension BridgeOperationRejection {
    /// Why the library refused a change this device asked for, as a
    /// sentence.
    public var label: String {
        switch self {
        case .storeOwner:
            String(
                localized:
                    "The account that owns the library's cloud home can't be removed."
            )
        case .personGone:
            String(localized: "That person is no longer in the library.")
        case .deviceGone:
            String(localized: "That device is no longer in the library.")
        case .circleGone: String(localized: "That circle no longer exists.")
        case .noAdminLeft:
            String(localized: "The library would have no admin left.")
        case .keysAlreadyUsed:
            String(localized: "Those keys are already in use.")
        case .memberKeysChanged:
            String(
                localized: "That person's keys were replaced in the meantime."
            )
        case .notAllowed:
            String(localized: "This device isn't allowed to make that change.")
        case .admissionTooLarge:
            String(
                localized:
                    "The library has changed its keys too many times to send them all to a new device."
            )
        }
    }
}
