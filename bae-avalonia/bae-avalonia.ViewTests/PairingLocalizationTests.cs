using System.Globalization;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class PairingLocalizationTests
{
    [Theory]
    [InlineData("en-US", "Scan the pairing code shown on a device already in your library.")]
    [InlineData("de-DE", "Scanne den Kopplungscode, der auf einem Gerät angezeigt wird, das bereits in deiner Mediathek ist.")]
    [InlineData("es-ES", "Escanea el código de emparejamiento que se muestra en un dispositivo que ya está en tu biblioteca.")]
    public void PairingInstructionsUseTheSelectedLocale(string locale, string expected)
    {
        var previous = CultureInfo.CurrentUICulture;
        CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo(locale);
        try
        {
            Assert.Equal(expected, Loc.Chrome("join.pairing_intro"));
        }
        finally
        {
            CultureInfo.CurrentUICulture = previous;
        }
    }
}
