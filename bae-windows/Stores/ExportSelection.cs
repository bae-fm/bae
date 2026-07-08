using uniffi.bae_bridge;

namespace Bae.Windows;

// Helpers over the bridge's export-selection union: the two constructors (export
// the original files, or apply a named preset) and value equality. The union has
// no structural equality, and the settings and export pickers compare a
// selection against the configured default to mark it.
internal static class ExportSelection
{
    public static BridgeExportSelection Original() => new BridgeExportSelection.Original();

    public static BridgeExportSelection Preset(string presetId) => new BridgeExportSelection.Preset(presetId);

    public static bool Equal(BridgeExportSelection a, BridgeExportSelection b) =>
        (a, b) switch
        {
            (BridgeExportSelection.Original, BridgeExportSelection.Original) => true,
            (BridgeExportSelection.Preset left, BridgeExportSelection.Preset right) => left.PresetId == right.PresetId,
            _ => false,
        };
}
