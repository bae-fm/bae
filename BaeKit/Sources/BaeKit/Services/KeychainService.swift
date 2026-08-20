import Foundation
import Security
import os.log

private let logger = Logger.bae("KeychainService")
// kCFBooleanTrue is typed as CFBoolean? in Swift but is an ObjC singleton constant,
// never nil. nonisolated(unsafe) because CFBoolean is non-Sendable, but the value
// is immutable — accessing it from any concurrency domain is safe.
nonisolated(unsafe) private let cfBoolTrue: CFBoolean = kCFBooleanTrue
    .unsafelyUnwrapped

/// A Keychain call that failed for a reason other than "no such item".
///
/// Carries the raw `OSStatus` because the interesting failures are the ones that
/// look like absence: `errSecInteractionNotAllowed` (-25308) means the keychain
/// is locked or the display is asleep — a refusal a later call succeeds at — and
/// folding it into an empty result told the user they had no restore codes at
/// all. Security's own message for the status is already localized by the OS, so
/// it is the line to show.
public struct KeychainFailure: Error, Equatable, LocalizedFailure {
    public let status: OSStatus

    public init(status: OSStatus) {
        self.status = status
    }

    public var localizedLine: String? {
        SecCopyErrorMessageString(status, nil) as String?
            ?? "OSStatus \(status)"
    }

    public var detail: String? { "OSStatus \(status)" }
}

/// Manages restore codes in iCloud Keychain for automatic cross-device library discovery.
///
/// Each library gets a separate keychain entry keyed by library ID. The
/// `kSecAttrSynchronizable` flag syncs entries via iCloud Keychain to other
/// Apple devices signed into the same Apple ID.
public enum KeychainService {
    private static let service = "fm.bae.restore"

    /// Save or update the restore code for the given library.
    ///
    /// `errSecItemNotFound` from the update is the ordinary first-write path and
    /// falls through to the add. Every other status throws: a restore code the
    /// keychain refused to store is a library the user cannot recover, and a log
    /// line is not somewhere they will ever look.
    public static func saveRestoreCode(libraryId: String, code: String) throws {
        let data = Data(code.utf8)

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: libraryId,
            kSecAttrSynchronizable as String: cfBoolTrue,
        ]

        // Try to update an existing entry first
        let updateAttributes: [String: Any] = [
            kSecValueData as String: data
        ]
        let updateStatus = SecItemUpdate(
            query as CFDictionary,
            updateAttributes as CFDictionary
        )
        if updateStatus == errSecSuccess {
            return
        }
        guard updateStatus == errSecItemNotFound else {
            throw KeychainFailure(status: updateStatus)
        }

        // No existing entry -- add a new one
        var addQuery = query
        addQuery[kSecValueData as String] = data
        addQuery[kSecAttrAccessible as String] =
            kSecAttrAccessibleAfterFirstUnlock
        let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw KeychainFailure(status: addStatus)
        }
    }

    /// Fetch all restore codes stored by any device.
    /// Returns an array of (libraryId, restoreCode) pairs.
    ///
    /// Only `errSecItemNotFound` is an empty list. Every other status throws,
    /// because the one that matters most is indistinguishable from absence
    /// otherwise: with the keychain locked or the display asleep, Security
    /// answers `errSecInteractionNotAllowed`, and reporting that as "no restore
    /// codes" put a first-run wall in front of a user who has libraries.
    public static func fetchAllRestoreCodes() throws -> [(
        libraryId: String, code: String
    )] {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrSynchronizable as String: cfBoolTrue,
            kSecMatchLimit as String: kSecMatchLimitAll,
            kSecReturnAttributes as String: cfBoolTrue,
            kSecReturnData as String: cfBoolTrue,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound {
            return []
        }
        guard status == errSecSuccess else {
            throw KeychainFailure(status: status)
        }
        guard let items = result as? [[String: Any]] else {
            // `kSecMatchLimitAll` with both return flags yields attribute
            // dictionaries on success. Another shape is this query being wrong,
            // not a condition the app can be right about.
            preconditionFailure(
                "Keychain returned an unreadable match set for restore codes"
            )
        }

        return items.compactMap { item in
            guard let account = item[kSecAttrAccount as String] as? String,
                let data = item[kSecValueData as String] as? Data,
                let code = String(data: data, encoding: .utf8)
            else {
                return nil
            }
            return (libraryId: account, code: code)
        }
    }

    /// Delete the restore code for a specific library.
    public static func deleteRestoreCode(libraryId: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: libraryId,
            kSecAttrSynchronizable as String: cfBoolTrue,
        ]

        let status = SecItemDelete(query as CFDictionary)
        if status != errSecSuccess, status != errSecItemNotFound {
            logger.error(
                "KeychainService: failed to delete restore code: \(status)"
            )
        }
    }
}
