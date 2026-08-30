using System;
using System.IO;

namespace Bae.Desktop;

// The device-local choice shared by Import and Move to Cloud: whether a release
// sent to cloud storage should also stay downloaded on this device.
internal static class StoragePinPreferenceStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "storage-pin.txt");

    public static bool Load()
    {
        try
        {
            if (File.Exists(FilePath)
                && bool.TryParse(File.ReadAllText(FilePath).Trim(), out var pinned))
            {
                return pinned;
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved cloud pin choice.", exception);
        }

        return true;
    }

    public static void Save(bool pinned)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, pinned.ToString());
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the cloud pin choice.", exception);
        }
    }
}
