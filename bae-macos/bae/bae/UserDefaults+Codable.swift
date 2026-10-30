import BaeKit
import Foundation
import OSLog

private let logger = Logger.bae("UserDefaults+Codable")

extension UserDefaults {
    /// Save `value` under `key` as its JSON encoding. A value that doesn't
    /// encode is logged and not saved, leaving what was saved before.
    func setEncoded<Value: Encodable>(_ value: Value, forKey key: String) {
        do {
            set(try JSONEncoder().encode(value), forKey: key)
        }
        catch {
            logger.error(
                "\(key) doesn't encode, not saved: \(String(reflecting: error))"
            )
        }
    }

    /// The value `setEncoded` saved under `key`, or `nil` when nothing is
    /// saved there. What is saved must be the JSON of a `Value`, whole: a
    /// value of another kind, or JSON that doesn't decode, is logged and
    /// read as nothing saved.
    func decoded<Value: Decodable>(
        _ type: Value.Type,
        forKey key: String
    ) -> Value? {
        guard let saved = object(forKey: key) else {
            return nil
        }
        guard let data = saved as? Data else {
            logger.error(
                "Saved \(key) is a \(Swift.type(of: saved)), not data; ignored"
            )
            return nil
        }
        do {
            return try JSONDecoder().decode(type, from: data)
        }
        catch {
            logger.error(
                "Saved \(key) doesn't decode, ignored: \(String(reflecting: error))"
            )
            return nil
        }
    }
}
