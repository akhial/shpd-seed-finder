using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace SeedSeeker;

/// <summary>The catalyst deck in its seeded order, using Fluent cards and pixel art.</summary>
public sealed class TrinketDeckView : StackPanel
{
    public TrinketDeckView(IReadOnlyList<CatalogItem> order, IReadOnlySet<string> matches, string? selectedTrinket, Action<string> onSelect)
    {
        Spacing = 10;
        Margin = new Thickness(0, 12, 0, 6);
        var choices = new Grid { ColumnSpacing = 6 };
        for (var i = 0; i < 4; i++) choices.ColumnDefinitions.Add(new ColumnDefinition());
        foreach (var (item, index) in order.Take(4).Select((item, index) => (item, index)))
        {
            var matched = matches.Contains(item.Id);
            var applied = selectedTrinket == item.Id;
            var card = new ToggleButton
            {
                CornerRadius = new CornerRadius(6), BorderThickness = new Thickness(applied ? 2 : 1),
                IsChecked = applied, Padding = new Thickness(0), HorizontalContentAlignment = HorizontalAlignment.Stretch,
                VerticalContentAlignment = VerticalAlignment.Stretch,
                Background = Resource(applied ? "SystemFillColorSuccessBackgroundBrush" : "CardBackgroundFillColorDefaultBrush"),
                BorderBrush = Resource(applied || matched ? "SystemFillColorSuccessBrush" : "CardStrokeColorDefaultBrush"),
            };
            AutomationProperties.SetName(card, item.Name + (applied ? ", applied at +3" : "") + (matched ? ", matches requirement" : ""));
            card.Click += (_, _) => { card.IsChecked = applied; onSelect(applied ? "none" : item.Id); };
            ToolTipService.SetToolTip(card, item.Name);
            var body = new Grid { Padding = new Thickness(5) };
            body.RowDefinitions.Add(new RowDefinition { Height = new GridLength(16) });
            body.RowDefinitions.Add(new RowDefinition());
            body.RowDefinitions.Add(new RowDefinition { Height = new GridLength(20) });
            var sprite = new SpriteView { SpriteIndex = item.SpriteIndex, SpriteSize = 48,
                HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            Grid.SetRow(sprite, 1); body.Children.Add(sprite);
            var name = new Viewbox { Stretch = Stretch.Uniform, StretchDirection = StretchDirection.DownOnly,
                HorizontalAlignment = HorizontalAlignment.Stretch, Margin = new Thickness(0, 2, 0, 2),
                Child = new TextBlock { Text = item.Name, FontSize = 12, TextWrapping = TextWrapping.NoWrap } };
            Grid.SetRow(name, 2); body.Children.Add(name);
            if (applied) body.Children.Add(new TextBlock { Text = "Applied +3", FontSize = 11,
                HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Top,
                Foreground = Resource("SystemFillColorSuccessBrush") });
            card.Content = body;
            card.SizeChanged += (_, _) =>
            {
                if (card.ActualWidth <= 0) return;
                card.Height = card.ActualWidth;
                sprite.SpriteSize = Math.Max(1, Math.Floor(Math.Min(48, Math.Min(card.ActualWidth - 12, card.ActualWidth - 50))));
            };
            Grid.SetColumn(card, index); choices.Children.Add(card);
        }
        Children.Add(choices);
        if (order.Count <= 4) return;
        Children.Add(new TextBlock { Text = "Remaining deck order", Style = (Style)Application.Current.Resources["Caption"] });
        var tail = new Grid { ColumnSpacing = 2 };
        foreach (var (item, index) in order.Skip(4).Select((item, index) => (item, index)))
        {
            tail.ColumnDefinitions.Add(new ColumnDefinition());
            var cell = new Grid { Height = 24 };
            var sprite = new SpriteView { SpriteIndex = item.SpriteIndex, SpriteSize = 24,
                HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            cell.Children.Add(sprite);
            ToolTipService.SetToolTip(cell, item.Name); AutomationProperties.SetName(cell, item.Name);
            cell.SizeChanged += (_, _) => sprite.SpriteSize = Math.Max(1, Math.Floor(Math.Min(24, cell.ActualWidth)));
            Grid.SetColumn(cell, index); tail.Children.Add(cell);
        }
        Children.Add(tail);
    }

    private static Brush Resource(string key) => (Brush)Application.Current.Resources[key];
}
