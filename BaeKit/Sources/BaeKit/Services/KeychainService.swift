import Foundation
import Security
import os.log

private let logger = Logger.bae("KeychainService")
// kCFBooleanTrue is typed as CFBoolean? in Swift but is an ObjC singleton constant,
// never nil. nonisolated(unsafe) because CFBoolean is non-Sendable, but the value
// is immutable — accessing it from any concurrency domain is safe.
nonisolated(unsafe) private let cfBoolTrue: CFBoolean = kCFBooleanTrue
    .unsafelyUnwrapped

/// Manages restore codes in iCloud Keychain for automatic cross-device library discovery.
///
/// Each library gets a separate keychain entry keyed by library ID. The
/// `kSecAttrSynchronizable` flag syncs entries via iCloud Keychain to other
/// Apple devices signed into the same Apple ID.
public enum KeychainService {
    private static let service = "fm.bae.restore"

    /// Save or update a restore code for the given library.
    /// Silently does nothing if iCloud Keychain is unavailable.
    public static func saveRestoreCode(libraryId: String, code: String) {
        guard let data = code.data(using: .utf8) else {
            return
        }

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

        if updateStatus == errSecItemNotFound {
            // No existing entry -- add a new one
            var addQuery = query
            addQuery[kSecValueData as String] = data
            addQuery[kSecAttrAccessible as String] =
                kSecAttrAccessibleAfterFirstUnlock
            let addStatus = SecItemAdd(addQuery as CFDictionary, nil)

            if addStatus != errSecSuccess {
                logger.error(
                    "KeychainService: failed to save restore code (add): \(addStatus)"
                )
            }
        }
        else if updateStatus != errSecSuccess {
            logger.error(
                "KeychainService: failed to save restore code (update): \(updateStatus)"
            )
        }
    }

    /// Fetch all restore codes stored by any device.
    /// Returns an array of (libraryId, restoreCode) pairs.
    public static func fetchAllRestoreCodes() -> [(
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

        guard status == errSecSuccess,
            let items = result as? [[String: Any]]
        else {
            return []
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
