using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Numerics;
using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI.Text;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.System;
using Windows.UI;

namespace SeedSeeker;

public sealed partial class MainWindow : Window
{
    private readonly NativeEngine engine = new();
    private readonly ObservableCollection<SeedResult> results = [];
    private QuerySettings query = new();
    private List<QueryPreset> userPresets = [];
    private NativeSearch? search;
    private bool restoring = true;
    private const int ResultCap = 1024;
    private static readonly string SettingsPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Seed Seeker", "query.json");
    private static readonly string PresetsPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Seed Seeker", "presets.json");

    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(nint hwnd);

    public MainWindow()
    {
        InitializeComponent();
        SystemBackdrop = new MicaBackdrop();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.TitleBar.PreferredHeightOption = TitleBarHeightOption.Tall;
        var scale = GetDpiForWindow(WinRT.Interop.WindowNative.GetWindowHandle(this)) / 96.0;
        AppWindow.Resize(new SizeInt32((int)(1180 * scale), (int)(720 * scale)));
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.PreferredMinimumWidth = (int)(1020 * scale);
            presenter.PreferredMinimumHeight = (int)(620 * scale);
        }
        ResultsList.ItemsSource = results; ScoutButton.IsEnabled = false;
        FloorSlider.Value = 1; FloorSlider.Minimum = 1; FloorSlider.Maximum = 24;
        LoadSettings(); LoadPresets(); RefreshPresets(); RefreshQuery();
        Closed += (_, _) => { search?.Cancel(); search?.Dispose(); };
    }

    private void LoadSettings()
    {
        restoring = true;
        try { if (File.Exists(SettingsPath)) query = JsonSerializer.Deserialize<QuerySettings>(File.ReadAllText(SettingsPath)) ?? new(); } catch { query = new(); }
        FloorSlider.Value = query.MaximumDepth; RequireBlacksmith.IsOn = query.RequireBlacksmith; ExcludeRewards.IsOn = query.ExcludeBlacksmithRewards; FastMode.IsOn = query.FastMode; restoring = false;
    }
    private void SaveSettings() { if (restoring) return; Directory.CreateDirectory(Path.GetDirectoryName(SettingsPath)!); File.WriteAllText(SettingsPath, JsonSerializer.Serialize(query, new JsonSerializerOptions { WriteIndented = true })); }
    private void LoadPresets()
    {
        try
        {
            if (File.Exists(PresetsPath))
                userPresets = (JsonSerializer.Deserialize<List<QueryPreset>>(File.ReadAllText(PresetsPath)) ?? [])
                    .Where(x => !string.IsNullOrWhiteSpace(x.Name) && x.Query is not null).ToList();
        }
        catch { userPresets = []; }
    }
    private void SavePresets()
    {
        Directory.CreateDirectory(Path.GetDirectoryName(PresetsPath)!);
        File.WriteAllText(PresetsPath, JsonSerializer.Serialize(userPresets, new JsonSerializerOptions { WriteIndented = true }));
    }
    private void RefreshPresets()
    {
        PresetPicker.ItemsSource = BuiltInPresets.All.Concat(userPresets).ToList();
        PresetPicker.SelectedIndex = -1; DeletePresetButton.IsEnabled = false;
    }
    private void ApplyQuery(QuerySettings value)
    {
        restoring = true; query = value.Clone();
        FloorSlider.Value = query.MaximumDepth; RequireBlacksmith.IsOn = query.RequireBlacksmith;
        ExcludeRewards.IsOn = query.ExcludeBlacksmithRewards; FastMode.IsOn = query.FastMode;
        restoring = false; RefreshQuery(); SaveSettings();
    }
    private void PresetPicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (PresetPicker.SelectedItem is not QueryPreset preset) { DeletePresetButton.IsEnabled = false; return; }
        ApplyQuery(preset.Query); DeletePresetButton.IsEnabled = !preset.IsBuiltIn;
    }
    private async void SavePreset_Click(object sender, RoutedEventArgs e)
    {
        var name = new TextBox { Header = "Preset name", PlaceholderText = "My preset", Width = 360 };
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Save Preset", PrimaryButtonText = "Save", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Primary, Content = name };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary || string.IsNullOrWhiteSpace(name.Text)) return;
        var cleanName = name.Text.Trim(); var existing = userPresets.FindIndex(x => string.Equals(x.Name, cleanName, StringComparison.OrdinalIgnoreCase));
        var preset = new QueryPreset { Name = cleanName, Query = query.Clone() };
        if (existing >= 0) { preset.Id = userPresets[existing].Id; userPresets[existing] = preset; } else userPresets.Add(preset);
        SavePresets(); RefreshPresets();
    }
    private void DeletePreset_Click(object sender, RoutedEventArgs e)
    {
        if (PresetPicker.SelectedItem is not QueryPreset { IsBuiltIn: false } preset) return;
        userPresets.RemoveAll(x => x.Id == preset.Id); SavePresets(); RefreshPresets();
    }
    private void RefreshQuery()
    {
        RequirementList.ItemsSource = query.Requirements; NoRequirements.Visibility = query.Requirements.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        FloorLabel.Text = $"first {query.MaximumDepth} floor{(query.MaximumDepth == 1 ? "" : "s")}"; RequireBlacksmith.IsEnabled = query.MaximumDepth < 14; StartButton.IsEnabled = search is not null || query.Requirements.Count != 0;
        var count = BitOperations.PopCount((uint)query.Challenges); ChallengeSummary.Text = count == 0 ? "None" : $"{count} enabled";
    }
    private void FloorSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e) { if (restoring || FloorLabel is null) return; query.MaximumDepth = (int)e.NewValue; RefreshQuery(); SaveSettings(); }
    private void SettingChanged(object sender, RoutedEventArgs e) { if (restoring) return; query.RequireBlacksmith = RequireBlacksmith.IsOn; query.ExcludeBlacksmithRewards = ExcludeRewards.IsOn; query.FastMode = FastMode.IsOn; SaveSettings(); }

    private async void AddRequirement_Click(object sender, RoutedEventArgs e) { var r = new ItemRequirement { Kind = ItemKind.Weapon, UpgradeMatch = UpgradeMatch.Any }; if (await EditRequirement(r, true)) { query.Requirements.Add(r); RefreshQuery(); SaveSettings(); } }
    private async void Requirement_Click(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is not ItemRequirement original) return; var copy = original.Clone();
        if (await EditRequirement(copy, false)) { var index = query.Requirements.IndexOf(original); query.Requirements[index] = copy; RefreshQuery(); SaveSettings(); }
    }
    private void RemoveRequirement_Click(object sender, RoutedEventArgs e) { if ((sender as Button)?.Tag is ItemRequirement r) { query.Requirements.Remove(r); RefreshQuery(); SaveSettings(); } }

    private async Task<bool> EditRequirement(ItemRequirement r, bool isNew)
    {
        var kind = Combo(Enum.GetValues<ItemKind>().Select(Labels.Kind), (int)r.Kind); kind.Header = "Category";
        var item = new ComboBox { Header = "Item", HorizontalAlignment = HorizontalAlignment.Stretch };
        var tierMatch = Combo(["Any tier", "Exactly", "At least", "At most"], (int)r.TierMatch); tierMatch.Header = "Tier predicate"; var selectedTier = r.Tier is >= 2 and <= 5 ? r.Tier : 2; var tier = Number("Tier", selectedTier, 2, 5); var tierBound = Combo(["Tier 3", "Tier 4"], Math.Clamp(selectedTier, 3, 4) - 3);
        var upgradeMatch = Combo(["Any", "Exactly", "At least"], (int)r.UpgradeMatch); upgradeMatch.Header = "Upgrade predicate"; var upgrade = Number("Upgrade level", r.Upgrade, 0, 4);
        var modifier = new ComboBox { Header = "Enchantment or glyph", HorizontalAlignment = HorizontalAlignment.Stretch };
        var uncursed = new CheckBox { Content = "Require uncursed", IsChecked = r.RequireUncursed };
        var source = Combo(new[] { "Any source" }.Concat(Enum.GetValues<ScoutItemSource>().Select(Labels.Source)), r.Source is null ? 0 : (int)r.Source + 1); source.Header = "Source";
        var group = Combo(["None", "A", "B", "C", "D"], r.IdentityGroup ?? 0); group.Header = "Same-item group";
        var depthToggle = ToggleRow("Limit this item to a floor", r.MaximumDepth is not null, out var depthRow); var depth = Number("Within first floors", r.MaximumDepth ?? 5, 1, 24);
        var content = new StackPanel { Spacing = 12, Padding = new Thickness(2, 4, 2, 4) }; foreach (var control in new UIElement[] { kind, item, tierMatch, tier, tierBound, upgradeMatch, upgrade, modifier, uncursed, source, group, depthRow, depth }) content.Children.Add(control);
        void NormalizeTier()
        {
            var predicate = (TierMatch)Math.Max(0, tierMatch.SelectedIndex);
            selectedTier = predicate is TierMatch.AtLeast or TierMatch.AtMost ? Math.Clamp(selectedTier, 3, 4) : Math.Clamp(selectedTier, 2, 5);
            tier.Value = selectedTier; tierBound.SelectedIndex = Math.Clamp(selectedTier, 3, 4) - 3;
        }
        void SyncVisibility()
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex); var generic = item.SelectedIndex == 0 && k is ItemKind.Weapon or ItemKind.Armor;
            var predicate = (TierMatch)Math.Max(0, tierMatch.SelectedIndex); var ranged = predicate is TierMatch.AtLeast or TierMatch.AtMost;
            tierMatch.Visibility = generic ? Visibility.Visible : Visibility.Collapsed;
            tier.Visibility = generic && predicate == TierMatch.Exactly ? Visibility.Visible : Visibility.Collapsed;
            tierBound.Visibility = generic && ranged ? Visibility.Visible : Visibility.Collapsed;
            tierBound.Header = predicate == TierMatch.AtLeast ? "Minimum tier" : "Maximum tier";
            upgrade.Visibility = upgradeMatch.SelectedIndex > 0 ? Visibility.Visible : Visibility.Collapsed;
        }
        void PopulateModifiers(string? selection)
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex);
            var modifiers = ItemCatalog.Modifiers(k)
                .Where(effect => uncursed.IsChecked != true || !ItemCatalog.IsCurse(k, effect))
                .ToList();
            modifier.Items.Clear(); modifier.Items.Add("None"); foreach (var value in modifiers) modifier.Items.Add(value);
            modifier.SelectedIndex = selection is null ? 0 : Math.Max(0, modifiers.IndexOf(selection) + 1);
            modifier.Visibility = k is ItemKind.Weapon or ItemKind.Armor ? Visibility.Visible : Visibility.Collapsed;
        }
        void Populate()
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex); var oldId = r.Item?.Id; var items = ItemCatalog.For(k).ToList(); item.Items.Clear(); item.Items.Add($"Any {Labels.Singular(k)}"); foreach (var value in items) item.Items.Add(value.Name); item.SelectedIndex = Math.Max(0, items.FindIndex(x => x.Id == oldId) + 1);
            PopulateModifiers(r.Modifier);
            upgrade.Maximum = k == ItemKind.Ring ? 4 : 3; SyncVisibility();
        }
        kind.SelectionChanged += (_, _) => { r.Item = null; r.Modifier = null; Populate(); }; item.SelectionChanged += (_, _) => SyncVisibility(); tier.ValueChanged += (_, _) => { if (!double.IsNaN(tier.Value)) selectedTier = (int)tier.Value; }; tierBound.SelectionChanged += (_, _) => { if (tierBound.SelectedIndex >= 0) selectedTier = tierBound.SelectedIndex + 3; }; tierMatch.SelectionChanged += (_, _) => { NormalizeTier(); SyncVisibility(); }; upgradeMatch.SelectionChanged += (_, _) => SyncVisibility(); uncursed.Checked += (_, _) => PopulateModifiers(modifier.SelectedItem is string effect && !ItemCatalog.IsCurse((ItemKind)Math.Max(0, kind.SelectedIndex), effect) ? effect : null); uncursed.Unchecked += (_, _) => PopulateModifiers(modifier.SelectedItem?.ToString()); depthToggle.Toggled += (_, _) => depth.Visibility = depthToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
        Populate(); NormalizeTier(); SyncVisibility(); depth.Visibility = depthToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = isNew ? "New Requirement" : "Edit Requirement", PrimaryButtonText = isNew ? "Add" : "Save", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Primary, Content = new ScrollViewer { Content = content, MaxHeight = 510, Width = 430 } };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return false;
        r.Kind = (ItemKind)kind.SelectedIndex; r.Item = item.SelectedIndex > 0 ? ItemCatalog.For(r.Kind).ElementAt(item.SelectedIndex - 1) : null; r.TierMatch = r.Item is null && r.Kind is ItemKind.Weapon or ItemKind.Armor ? (TierMatch)tierMatch.SelectedIndex : TierMatch.Any; r.Tier = r.TierMatch == TierMatch.Any ? 0 : selectedTier;
        r.UpgradeMatch = (UpgradeMatch)upgradeMatch.SelectedIndex; r.Upgrade = r.UpgradeMatch == UpgradeMatch.Any ? 0 : Math.Max(r.UpgradeMatch == UpgradeMatch.Exactly ? 1 : 0, (int)upgrade.Value); r.Modifier = modifier.Visibility == Visibility.Visible && modifier.SelectedIndex > 0 ? modifier.SelectedItem?.ToString() : null;
        r.RequireUncursed = uncursed.IsChecked == true; r.Source = source.SelectedIndex == 0 ? null : (ScoutItemSource)(source.SelectedIndex - 1); r.IdentityGroup = group.SelectedIndex == 0 ? null : group.SelectedIndex; r.MaximumDepth = depthToggle.IsOn ? (int)depth.Value : null; return true;
    }
    private static ComboBox Combo(IEnumerable<string> values, int selected) { var c = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch }; foreach (var v in values) c.Items.Add(v); c.SelectedIndex = selected; return c; }
    private static NumberBox Number(string header, double value, double min, double max) => new() { Header = header, Value = value, Minimum = min, Maximum = max, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Compact };
    private static ToggleSwitch ToggleRow(string label, bool isOn, out Grid row)
    {
        var toggle = new ToggleSwitch { IsOn = isOn, MinWidth = 0, Width = 44, OnContent = "", OffContent = "", Margin = new Thickness(0, -6, 0, -6), VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Right };
        row = new Grid { ColumnSpacing = 12 }; row.ColumnDefinitions.Add(new ColumnDefinition()); row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var text = new TextBlock { Text = label, VerticalAlignment = VerticalAlignment.Center }; Grid.SetColumn(toggle, 1); row.Children.Add(text); row.Children.Add(toggle);
        return toggle;
    }

    private async void Challenges_Click(object sender, RoutedEventArgs e)
    {
        var entries = new (int Mask, string Name, bool Changes)[] { (1,"On diet",false),(2,"Faith is my armor",false),(4,"Pharmacophobia",false),(8,"Barren land",true),(16,"Swarm intelligence",false),(32,"Into darkness",true),(64,"Forbidden runes",true),(128,"Hostile champions",false),(256,"Badder bosses",false) };
        var secondary = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
        var panel = new StackPanel { Width = 400 }; var toggles = new List<(int, ToggleSwitch)>();
        panel.Children.Add(new TextBlock { Text = "Searches simulate runs with the selected challenges enabled.", TextWrapping = TextWrapping.Wrap, Foreground = secondary, Margin = new Thickness(0, 0, 0, 6) });
        foreach (var entry in entries)
        {
            var row = new Grid { ColumnSpacing = 12, Padding = new Thickness(0, 8, 0, 8) };
            row.ColumnDefinitions.Add(new ColumnDefinition()); row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var text = new StackPanel { Spacing = 1 };
            text.Children.Add(new TextBlock { Text = entry.Name });
            text.Children.Add(new TextBlock { Text = entry.Changes ? "changes level generation" : "no effect on seed content", FontSize = 12, Foreground = secondary });
            var toggle = new ToggleSwitch { IsOn = (query.Challenges & entry.Mask) != 0, MinWidth = 0, Width = 44, OnContent = "", OffContent = "", Margin = new Thickness(0, -6, 0, -6), VerticalAlignment = VerticalAlignment.Center };
            Grid.SetColumn(toggle, 1); row.Children.Add(text); row.Children.Add(toggle); panel.Children.Add(row); toggles.Add((entry.Mask, toggle));
        }
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Challenges", PrimaryButtonText = "Done", DefaultButton = ContentDialogButton.Primary, Content = new ScrollViewer { Content = panel, MaxHeight = 520 } };
        await dialog.ShowAsync(); query.Challenges = toggles.Where(x => x.Item2.IsOn).Aggregate(0, (mask, x) => mask | x.Item1); RefreshQuery(); SaveSettings();
    }

    private async void Start_Click(object sender, RoutedEventArgs e)
    {
        if (search is not null) { search.Cancel(); StartButton.IsEnabled = false; return; } results.Clear(); SearchStatus.Text = "Starting search…"; SetStartButton(running: true);
        try { search = await Task.Run(() => engine.Start(query)); await RunSearch(search); } catch (Exception ex) { SearchStatus.Text = $"Failed: {ex.Message}"; }
        finally { search?.Dispose(); search = null; SetStartButton(running: false); StartButton.IsEnabled = query.Requirements.Count != 0; }
    }
    private void SetStartButton(bool running)
    {
        StartIcon.Glyph = running ? "" : "";
        StartLabel.Text = running ? "Cancel Search" : "Start Search";
        PresetPicker.IsEnabled = !running;
        SavePresetButton.IsEnabled = !running;
        DeletePresetButton.IsEnabled = !running
            && PresetPicker.SelectedItem is QueryPreset { IsBuiltIn: false };
    }
    private async Task RunSearch(NativeSearch active)
    {
        var timer = Stopwatch.StartNew(); long lastScanned = 0; var lastTime = 0d;
        while (true)
        {
            await Task.Delay(150); var pollCount = Math.Max(1, Math.Min(128, ResultCap - results.Count)); var batch = await Task.Run(() => active.Poll(pollCount)); foreach (var seed in batch) if (results.Count < ResultCap) results.Add(new(seed, results.Count + 1));
            var status = await Task.Run(active.Status); var seconds = timer.Elapsed.TotalSeconds; var rate = seconds > lastTime ? (status.Scanned - lastScanned) / (seconds - lastTime) : 0; lastScanned = status.Scanned; lastTime = seconds;
            var probability = status.Probability > 0 ? $"{status.Probability:P4}" : "calculating"; var tts = status.Probability > 0 && rate > 0 ? FormatDuration(1 / status.Probability / rate) : "calculating";
            SearchStatus.Text = status.State == SearchState.Running ? $"Seed match probability: {probability}   •   TTS @ {rate:N0} seeds/s: {tts}\nTime elapsed: {FormatDuration(seconds)}" : status.State switch { SearchState.Completed => "Completed", SearchState.Cancelled => "Cancelled", _ => $"Failed (error {status.ErrorCode})" };
            if (results.Count >= ResultCap) { active.Cancel(); SearchStatus.Text += "\nResult limit reached (1,024 seeds)."; } if (status.State != SearchState.Running || results.Count >= ResultCap) break;
        }
    }
    private static string FormatDuration(double seconds) => seconds switch { < 1 => "less than a second", < 60 => $"{seconds:N0}s", < 3600 => $"{seconds / 60:N1}m", < 86400 => $"{seconds / 3600:N1}h", _ => $"{seconds / 86400:N1}d" };

    private void ResultsList_SelectionChanged(object sender, SelectionChangedEventArgs e) { if (ResultsList.SelectedItem is SeedResult row) { SeedInput.Text = row.Seed; _ = ScoutSeed(row.Seed); } }
    private void ResultsList_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e) { if (ResultsList.SelectedItem is SeedResult row) Copy(row.Seed); }
    private void CopyResult_Click(object sender, RoutedEventArgs e) { if ((sender as FrameworkElement)?.DataContext is SeedResult row) Copy(row.Seed); }
    private void ScoutResult_Click(object sender, RoutedEventArgs e) { if ((sender as FrameworkElement)?.DataContext is SeedResult row) { SeedInput.Text = row.Seed; _ = ScoutSeed(row.Seed); } }
    private void SeedInput_TextChanged(object sender, TextChangedEventArgs e) { var formatted = SeedCode.Format(SeedInput.Text); if (formatted != SeedInput.Text) { SeedInput.Text = formatted; SeedInput.SelectionStart = formatted.Length; } ScoutButton.IsEnabled = SeedCode.IsCanonical(formatted); }
    private void SeedInput_KeyDown(object sender, KeyRoutedEventArgs e) { if (e.Key == VirtualKey.Enter && SeedCode.IsCanonical(SeedInput.Text)) { _ = ScoutSeed(SeedInput.Text); e.Handled = true; } }
    private async void Scout_Click(object sender, RoutedEventArgs e) => await ScoutSeed(SeedInput.Text);
    private async Task ScoutSeed(string seed)
    {
        ScoutButton.IsEnabled = false; ScoutStatus.Text = "Scouting…";
        try
        {
            var world = await Task.Run(() => engine.Scout(seed, query.Challenges));
            var matches = ScoutMatcher.SelectMatches(world.Items, query.Requirements,
                query.MaximumDepth, query.ExcludeBlacksmithRewards);
            var groups = world.Items.Select((item, index) => (Item: item, Index: index))
                .GroupBy(x => x.Item.Depth).OrderBy(g => g.Key).Select(g =>
            {
                var group = new ScoutGroup { Floor = $"Floor {g.Key}", Region = Region(g.Key) };
                group.AddRange(g.Select(entry => ScoutRow.From(entry.Item, matches.Contains(entry.Index)))); return group;
            }).ToList();
            ScoutList.ItemsSource = new CollectionViewSource { IsSourceGrouped = true, Source = groups }.View;
            ScoutStatus.Text = $"{world.Items.Count} items across {groups.Count} floors" + (query.Requirements.Count == 0 ? "" : $"  ·  {matches.Count} requirement match{(matches.Count == 1 ? "" : "es")}");
            EmptyScout.Visibility = Visibility.Collapsed; ScoutList.Visibility = Visibility.Visible;
        }
        catch (Exception ex) { ScoutStatus.Text = ex.Message; } finally { ScoutButton.IsEnabled = SeedCode.IsCanonical(SeedInput.Text); }
    }
    private static string Region(int depth) => depth switch { <= 5 => "Sewers", <= 10 => "Prison", <= 15 => "Caves", <= 20 => "Dwarven City", _ => "Demon Halls" };
    private void CopySeed_Click(object sender, RoutedEventArgs e) { if (SeedCode.IsCanonical(SeedInput.Text)) Copy(SeedInput.Text); }
    private static void Copy(string text) { var data = new DataPackage(); data.SetText(text); Clipboard.SetContent(data); }
}

public sealed class ScoutGroup : List<ScoutRow>
{
    public string Floor { get; init; } = "";
    public string Region { get; init; } = "";
}

public sealed class ScoutRow
{
    public string ItemName { get; init; } = "";
    public string Upgrade { get; init; } = "";
    public Visibility UpgradeVisibility { get; init; } = Visibility.Collapsed;
    public Visibility CurseVisibility { get; init; } = Visibility.Collapsed;
    public string Effect { get; init; } = "";
    public Visibility EffectVisibility { get; init; } = Visibility.Collapsed;
    public Brush EffectBrush { get; init; } = new SolidColorBrush(Color.FromArgb(255, 42, 160, 176));
    public string Source { get; init; } = "";
    public string Accessibility { get; init; } = "";
    public Visibility AccessibilityVisibility { get; init; } = Visibility.Collapsed;
    public Visibility MatchVisibility { get; init; } = Visibility.Collapsed;
    public string Glyph { get; init; } = "";
    public Brush Tint { get; init; } = new SolidColorBrush(Microsoft.UI.Colors.Gray);
    public Windows.UI.Text.FontWeight Weight { get; init; } = FontWeights.Normal;

    public static ScoutRow From(ScoutItem x, bool match)
    {
        var access = x.AccessibilityTag switch { 1 => $"One reward of choice group {x.AccessibilityGroup} (option {x.AccessibilityValue + 1})", 2 => $"Only in some outcomes of scenario group {x.AccessibilityGroup}", _ => "" };
        var isCurse = x.Effect is not null && ItemCatalog.IsCurse(x.Item.Kind, x.Effect);
        return new()
        {
            ItemName = x.Item.Name,
            Upgrade = $"+{x.Upgrade}", UpgradeVisibility = x.Upgrade > 0 ? Visibility.Visible : Visibility.Collapsed,
            CurseVisibility = x.Cursed ? Visibility.Visible : Visibility.Collapsed,
            Effect = x.Effect ?? "", EffectVisibility = x.Effect is null ? Visibility.Collapsed : Visibility.Visible,
            EffectBrush = isCurse ? (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"] : new SolidColorBrush(Color.FromArgb(255, 42, 160, 176)),
            Source = Labels.Source(x.Source),
            Accessibility = access, AccessibilityVisibility = access.Length == 0 ? Visibility.Collapsed : Visibility.Visible,
            MatchVisibility = match ? Visibility.Visible : Visibility.Collapsed,
            Weight = match ? FontWeights.SemiBold : FontWeights.Normal,
            Glyph = KindStyle.Glyph(x.Item.Kind), Tint = KindStyle.Tint(x.Item.Kind),
        };
    }
}
