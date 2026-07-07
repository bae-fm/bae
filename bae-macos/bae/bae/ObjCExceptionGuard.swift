import Foundation

enum ObjCExceptionGuard {
    static func value(forKey key: String, on object: AnyObject) -> Any? {
        BaeObjCExceptionGuard.value(forKey: key, on: object)
    }
}
