import AppKit
import ObjectiveC

/// The display every scene shown in its window is drawn on: a virtual display
/// of 1600 by 1200 points at 2 pixels a point, beside whatever displays the
/// Mac has, which a shot test adds and hands to each render. A shot taken from a real display
/// would depend on that display: a small display leaves less free area than the
/// library window's scenes need, and a display at 1 pixel a
/// point would back the window at half the shot's scale. On this display
/// every Mac draws a window scene the same way, and the window server still
/// composes the window with the alert panels the app raises over it.
///
/// CoreGraphics adds a virtual display only through its private classes
/// `CGVirtualDisplayDescriptor`, `CGVirtualDisplayMode`,
/// `CGVirtualDisplaySettings` and `CGVirtualDisplay`, which no SDK header
/// declares, so this type reaches them through the Objective-C runtime. It is
/// the only code that does. When macOS lacks them, or makes no display from
/// them, every window scene fails with `ShotError.noShotDisplay`.
@MainActor
final class ShotDisplay {
    /// The display's size, in points.
    static let size = CGSize(width: 1_600, height: 1_200)
    /// The display draws at the scale every shot is captured at.
    private static let scale = ShotRenderer.scale

    let id: CGDirectDisplayID
    /// The `CGVirtualDisplay`; the display lasts as long as this object.
    private let display: NSObject

    private init(id: CGDirectDisplayID, display: NSObject) {
        self.id = id
        self.display = display
    }

    /// The screen AppKit lists for the display, with its free area.
    func screen() throws -> NSScreen {
        guard
            let screen = NSScreen.screens.first(where: {
                Self.number(of: $0) == id
            })
        else {
            throw ShotError.noShotDisplay("AppKit no longer lists it")
        }
        return screen
    }

    private static func number(of screen: NSScreen) -> CGDirectDisplayID? {
        screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")]
            as? CGDirectDisplayID
    }

    /// Add the display, once AppKit lists it at its size and scale.
    static func add() async throws -> ShotDisplay {
        let width = Int(size.width)
        let height = Int(size.height)
        let display = try virtualDisplay(
            pixelsWide: width * scale,
            pixelsHigh: height * scale,
            mode: (width, height)
        )
        guard
            let id = display.value(forKey: "displayID") as? CGDirectDisplayID,
            id != 0
        else {
            throw ShotError.noShotDisplay("CoreGraphics gave it no display ID")
        }
        // CoreGraphics lists the display's modes once the display is
        // connected, which AppKit announces by listing its screen.
        try await listed(id, as: "a screen") { _ in true }
        // The display starts in a mode of 1 pixel a point; its modes include
        // the same size at 2.
        let options =
            [kCGDisplayShowDuplicateLowResolutionModes: true] as CFDictionary
        guard
            let modes = CGDisplayCopyAllDisplayModes(id, options)
                as? [CGDisplayMode]
        else {
            throw ShotError.noShotDisplay("CoreGraphics lists no modes for it")
        }
        guard
            let mode = modes.first(where: {
                $0.width == width && $0.height == height
                    && $0.pixelWidth == width * scale
                    && $0.pixelHeight == height * scale
            })
        else {
            throw ShotError.noShotDisplay(
                "it offers no mode of \(width) by \(height) points at "
                    + "\(scale) pixels a point"
            )
        }
        let status = CGDisplaySetDisplayMode(id, mode, nil)
        guard status == .success else {
            throw ShotError.noShotDisplay(
                "CoreGraphics refused its mode with error \(status.rawValue)"
            )
        }
        try await listed(
            id,
            as: "\(width) by \(height) points at \(scale) pixels a point"
        ) {
            $0.backingScaleFactor == CGFloat(scale) && $0.frame.size == size
        }
        return ShotDisplay(id: id, display: display)
    }

    /// Wait for AppKit to list display `id` with a screen that `holds`, which
    /// `described` names in the failure.
    private static func listed(
        _ id: CGDirectDisplayID,
        as described: String,
        where holds: @MainActor (NSScreen) -> Bool
    ) async throws {
        do {
            try await Wait.until(
                notices: [
                    Wait.Notice(
                        NSApplication.didChangeScreenParametersNotification,
                        of: nil
                    )
                ]
            ) {
                NSScreen.screens.contains { number(of: $0) == id && holds($0) }
            }
        }
        catch is Wait.TimedOut {
            throw ShotError.noShotDisplay(
                "AppKit never listed it as \(described)"
            )
        }
    }

    /// A `CGVirtualDisplay` of at most `pixelsWide` by `pixelsHigh`, offering
    /// `mode` in points.
    private static func virtualDisplay(
        pixelsWide: Int,
        pixelsHigh: Int,
        mode: (width: Int, height: Int)
    ) throws -> NSObject {
        let displayClass = try privateClass("CGVirtualDisplay")
        typealias InitWithDescriptor =
            @convention(c) (AnyObject, Selector, AnyObject) -> Unmanaged<
                NSObject
            >?
        guard
            let display = try call(
                "initWithDescriptor:",
                of: displayClass,
                as: InitWithDescriptor.self
            )(
                try allocate(displayClass),
                NSSelectorFromString("initWithDescriptor:"),
                try descriptor(pixelsWide: pixelsWide, pixelsHigh: pixelsHigh)
            )?
            .takeRetainedValue()
        else {
            throw ShotError.noShotDisplay(
                "CoreGraphics made no display from its descriptor"
            )
        }
        typealias ApplySettings =
            @convention(c) (AnyObject, Selector, AnyObject) -> Bool
        let applied = try call(
            "applySettings:",
            of: displayClass,
            as: ApplySettings.self
        )(
            display,
            NSSelectorFromString("applySettings:"),
            try settings(offering: mode)
        )
        guard applied else {
            throw ShotError.noShotDisplay("CoreGraphics refused its settings")
        }
        return display
    }

    /// A `CGVirtualDisplayDescriptor` of at most `pixelsWide` by
    /// `pixelsHigh`.
    private static func descriptor(
        pixelsWide: Int,
        pixelsHigh: Int
    ) throws -> NSObject {
        let descriptor = try privateClass("CGVirtualDisplayDescriptor").init()
        descriptor.setValue(DispatchQueue.main, forKey: "queue")
        descriptor.setValue("bae shots", forKey: "name")
        descriptor.setValue(pixelsWide, forKey: "maxPixelsWide")
        descriptor.setValue(pixelsHigh, forKey: "maxPixelsHigh")
        // The physical size the descriptor asks for.
        descriptor.setValue(
            NSValue(size: CGSize(width: 600, height: 450)),
            forKey: "sizeInMillimeters"
        )
        descriptor.setValue(0xBAE, forKey: "vendorID")
        descriptor.setValue(0xBAE, forKey: "productID")
        descriptor.setValue(1, forKey: "serialNum")
        return descriptor
    }

    /// `CGVirtualDisplaySettings` that offer `mode` in points, at 1 and at 2
    /// pixels a point.
    private static func settings(
        offering mode: (width: Int, height: Int)
    ) throws -> NSObject {
        let modeClass = try privateClass("CGVirtualDisplayMode")
        typealias InitWithSize =
            @convention(c) (AnyObject, Selector, UInt, UInt, Double) ->
            Unmanaged<NSObject>
        let pointMode = try call(
            "initWithWidth:height:refreshRate:",
            of: modeClass,
            as: InitWithSize.self
        )(
            try allocate(modeClass),
            NSSelectorFromString("initWithWidth:height:refreshRate:"),
            UInt(mode.width),
            UInt(mode.height),
            60
        )
        .takeRetainedValue()

        let settings = try privateClass("CGVirtualDisplaySettings").init()
        settings.setValue([pointMode], forKey: "modes")
        // Offer each mode at 2 pixels a point too.
        settings.setValue(1, forKey: "hiDPI")
        return settings
    }

    private static func privateClass(_ name: String) throws -> NSObject.Type {
        guard let found = NSClassFromString(name) as? NSObject.Type else {
            throw ShotError.noShotDisplay("macOS has no class \(name)")
        }
        return found
    }

    private static func allocate(_ type: NSObject.Type) throws -> AnyObject {
        guard
            let allocated = type.perform(NSSelectorFromString("alloc"))?
                .takeUnretainedValue()
        else {
            throw ShotError.noShotDisplay("\(type) allocated nothing")
        }
        return allocated
    }

    /// The instance method `selector` of `type`, as a function of `signature`.
    private static func call<Function>(
        _ selector: String,
        of type: NSObject.Type,
        as signature: Function.Type
    ) throws -> Function {
        let selector = NSSelectorFromString(selector)
        guard
            type.instancesRespond(to: selector),
            let implementation = class_getMethodImplementation(type, selector)
        else {
            throw ShotError.noShotDisplay(
                "\(type) has no method \(NSStringFromSelector(selector))"
            )
        }
        return unsafeBitCast(implementation, to: signature)
    }
}
