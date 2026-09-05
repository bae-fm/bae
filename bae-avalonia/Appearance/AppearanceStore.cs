using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bae.Desktop;

internal enum AppearanceMode { System, Light, Dark }
internal enum AccentChoice { Blue, Indigo, Purple, Pink, Red, Amber, Green, Teal }
internal enum SurfaceTone { Neutral, Slate, Plum, Midnight, Forest, Sand }

internal sealed record AppearancePreferences(
    [property: JsonRequired] AppearanceMode Mode,
    [property: JsonRequired] AccentChoice Accent,
    [property: JsonRequired] SurfaceTone Tone)
{
    internal static AppearancePreferences Default { get; } =
        new(AppearanceMode.System, AccentChoice.Blue, SurfaceTone.Neutral);
}

// Saving commits the complete preference before notifying readers. A refused
// write leaves the displayed selection and the previous file intact.
internal sealed class AppearanceStore(
    AppearancePreferences initial, Action<AppearancePreferences> persist)
{
    internal AppearancePreferences Current { get; private set; } = initial;
    internal event Action? Changed;

    internal void Set(AppearancePreferences preferences)
    {
        persist(preferences);
        Current = preferences;
        Changed?.Invoke();
    }

    internal static AppearanceStore FromFile(string path)
    {
        var options = new JsonSerializerOptions
        {
            Converters = { new JsonStringEnumConverter(JsonNamingPolicy.CamelCase, allowIntegerValues: false) },
        };
        var initial = File.Exists(path)
            ? JsonSerializer.Deserialize<AppearancePreferences>(File.ReadAllText(path), options)
                ?? throw new InvalidDataException("Appearance preferences are empty")
            : AppearancePreferences.Default;
        return new AppearanceStore(initial, preferences =>
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            var temporary = path + ".tmp";
            try
            {
                File.WriteAllText(temporary, JsonSerializer.Serialize(preferences, options));
                File.Move(temporary, path, overwrite: true);
            }
            finally
            {
                if (File.Exists(temporary))
                {
                    File.Delete(temporary);
                }
            }
        });
    }
}
