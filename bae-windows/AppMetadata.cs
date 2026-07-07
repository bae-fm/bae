using System.Reflection;

namespace Bae.Windows;

/// <summary>
/// Build-time values baked into the assembly as
/// <see cref="AssemblyMetadataAttribute"/> entries (the Datadog / Sentry
/// credentials, the environment, and the git commit). Returns null when the
/// value is blank or still an unexpanded MSBuild placeholder (<c>$(...)</c>), so
/// a build that did not supply one reads as absent rather than as the literal
/// token.
/// </summary>
internal static class AppMetadata
{
    private static readonly Assembly AppAssembly = Assembly.GetExecutingAssembly();

    internal static string? ConfiguredString(string name)
    {
        var value = AppAssembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(attribute => attribute.Key == name)
            ?.Value;
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        var trimmed = value.Trim();
        return trimmed.StartsWith("$(", StringComparison.Ordinal) ? null : trimmed;
    }
}
