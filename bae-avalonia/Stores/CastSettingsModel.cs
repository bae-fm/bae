namespace Bae.Desktop;

// The pure decision layer behind the settings Casting section, mirroring macOS's
// CastingSettingsTab.toggleAction. Plain BCL types so it is unit-tested apart
// from the dialog that renders it.
public static class CastSettingsModel
{
    // Whether flipping the toggle to <paramref name="enabled"/> should ask first.
    // Turning casting off ends the session in flight, so that one case is
    // confirmed; every other flip writes straight through.
    public static bool NeedsDisconnectConfirmation(bool enabled, string? castingDeviceName) =>
        !enabled && castingDeviceName is not null;
}
