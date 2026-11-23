import AppKit

extension NSScreen {
    var displayNumber: CGDirectDisplayID? {
        deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")]
            as? CGDirectDisplayID
    }
}
