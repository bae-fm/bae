using Xunit;

namespace Bae.Desktop.Tests;

// The settings Casting section's pure decision layer: only turning casting off
// mid-session asks first, because that write is what ends the session.
public sealed class CastSettingsModelTests
{
    [Fact]
    public void TurningOn_NeverAsks()
    {
        Assert.False(CastSettingsModel.NeedsDisconnectConfirmation(enabled: true, castingDeviceName: null));
        Assert.False(
            CastSettingsModel.NeedsDisconnectConfirmation(enabled: true, castingDeviceName: "Living Room Speaker"));
    }

    [Fact]
    public void TurningOff_WhileIdle_NeverAsks()
    {
        Assert.False(CastSettingsModel.NeedsDisconnectConfirmation(enabled: false, castingDeviceName: null));
    }

    [Fact]
    public void TurningOff_MidSession_Asks()
    {
        Assert.True(
            CastSettingsModel.NeedsDisconnectConfirmation(enabled: false, castingDeviceName: "Living Room Speaker"));
    }
}
