// SPDX-License-Identifier: GPL-3.0-or-later
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Foundation;

namespace SeedSeeker;

/// <summary>
/// Lays children out left to right at their natural size, starting a new row
/// whenever the next child would overflow the available width. WinUI ships no
/// wrapping panel of its own (<c>ItemsWrapGrid</c> and <c>UniformGridLayout</c>
/// both force a uniform cell), and the quest chips are all different widths.
/// </summary>
public sealed class WrapPanel : Panel
{
    /// <summary>Gap between two children on the same row.</summary>
    public double Spacing { get; set; }
    /// <summary>Gap between two rows.</summary>
    public double LineSpacing { get; set; }

    protected override Size MeasureOverride(Size availableSize)
    {
        var line = new Size(0, 0);
        var total = new Size(0, 0);
        foreach (var child in Children)
        {
            child.Measure(new Size(availableSize.Width, double.PositiveInfinity));
            var desired = child.DesiredSize;
            // A row always keeps its first child, however wide it is.
            if (line.Width > 0 && line.Width + Spacing + desired.Width > availableSize.Width)
            {
                total = new Size(Math.Max(total.Width, line.Width), total.Height + line.Height + LineSpacing);
                line = new Size(0, 0);
            }
            line = new Size(
                line.Width == 0 ? desired.Width : line.Width + Spacing + desired.Width,
                Math.Max(line.Height, desired.Height));
        }
        return new Size(Math.Max(total.Width, line.Width), total.Height + line.Height);
    }

    protected override Size ArrangeOverride(Size finalSize)
    {
        double x = 0, y = 0, lineHeight = 0;
        foreach (var child in Children)
        {
            var desired = child.DesiredSize;
            if (x > 0 && x + desired.Width > finalSize.Width)
            {
                x = 0;
                y += lineHeight + LineSpacing;
                lineHeight = 0;
            }
            child.Arrange(new Rect(x, y, desired.Width, desired.Height));
            x += desired.Width + Spacing;
            lineHeight = Math.Max(lineHeight, desired.Height);
        }
        return new Size(finalSize.Width, y + lineHeight);
    }
}
