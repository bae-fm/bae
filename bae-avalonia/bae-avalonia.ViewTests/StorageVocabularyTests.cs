using System;
using System.Globalization;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class StorageVocabularyTests
{
    [Theory]
    [InlineData(false, false, "Local")]
    [InlineData(false, true, "Local")]
    [InlineData(true, false, "Cloud")]
    [InlineData(true, true, "Pinned")]
    public void RestingLabelsUseStorageVocabulary(bool cloud, bool pinned, string expected) =>
        Assert.Equal(expected, InEnglish(() => DialogPrimitives.RestingStorageLabel(cloud, pinned)));

    [Theory]
    [InlineData("cloud", "Move to Cloud")]
    [InlineData("local", "Make Local…")]
    [InlineData("pin", "Pin")]
    [InlineData("unpin", "Unpin")]
    public void ActionLabelsUseStorageVocabulary(string actionName, string expected)
    {
        var action = actionName switch
        {
            "cloud" => BridgeReleaseStorageAction.MakeRemote,
            "local" => BridgeReleaseStorageAction.MakeLocal,
            "pin" => BridgeReleaseStorageAction.Pin,
            "unpin" => BridgeReleaseStorageAction.Unpin,
            _ => throw new ArgumentOutOfRangeException(nameof(actionName), actionName, null),
        };
        Assert.Equal(expected, InEnglish(() => DialogPrimitives.StorageActionLabel(action)));
    }

    private static string InEnglish(Func<string> action)
    {
        var previousCulture = CultureInfo.CurrentCulture;
        var previousUiCulture = CultureInfo.CurrentUICulture;
        var english = CultureInfo.GetCultureInfo("en-US");
        CultureInfo.CurrentCulture = english;
        CultureInfo.CurrentUICulture = english;
        try
        {
            return action();
        }
        finally
        {
            CultureInfo.CurrentCulture = previousCulture;
            CultureInfo.CurrentUICulture = previousUiCulture;
        }
    }
}
