// The generated uniffi bindings (AppHandle, the Bridge* records/enums, the
// bridge free functions) live in the BaeBridge target so the same generated
// Swift can be paired with the platform-conditional binary xcframework. Both
// apps use those types pervasively, so re-export them from BaeKit — one
// `import BaeKit` then sees the shared data layer and the bridge types together.
@_exported import BaeBridge
