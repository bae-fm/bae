// SwiftPM resolves package targets before Xcode runs scheme pre-actions. Keep
// the target present so that pre-action can install its generated bindings
// before Xcode plans compilation.
enum BaeBridgeBootstrap {}
