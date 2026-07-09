using System;
using Microsoft.UI.Xaml.Data;

namespace Bae.Windows;

// x:Bind has no built-in bool -> double conversion, so the album card's
// always-present selection tint (opacity 1/0, never added/removed from the
// layout tree) needs this to bind its Opacity straight to Album.IsSelected.
public sealed class BoolToOpacityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        value is true ? 1.0 : 0.0;

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}
