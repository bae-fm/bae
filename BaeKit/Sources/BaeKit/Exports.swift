// The generated uniffi bindings (AppHandle, the Bridge* records/enums, the
// bridge free functions) live in the BaeBridge target, beside the binary
// xcframework whose slice for each platform they call into. Both
// apps use those types pervasively, so re-export them from BaeKit — one
// `import BaeKit` then sees the shared data layer and the bridge types together.
@_exported import BaeBridge
