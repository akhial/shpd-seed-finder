using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Numerics;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text.Json;
using Microsoft.UI.Text;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Graphics.Imaging;
using Windows.Storage;
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
    /// <summary>The last concluded run's record — its query, delivered seeds, and
    /// resume position. Consulted only when <see cref="lastRunDetached"/> is set,
    /// to let a query continue the previous detached scan instead of rescanning;
    /// every related query works from <see cref="target"/> instead.</summary>
    private BaseRun? baseRun;
    /// <summary>The session's Target — the first concluded (or imported) search's
    /// query, its full uncapped seed set, and its unscanned coverage. Related
    /// queries refine or filter it; only Clear Results discards it. See
    /// docs/search-semantics.md.</summary>
    private TargetRun? target;
    /// <summary>True when the run recorded in <see cref="baseRun"/> was a detached
    /// scan — the only run an unrelated query may implicitly continue.</summary>
    private bool lastRunDetached;
    /// <summary>True for the whole span of a search or refine, including the
    /// refine's filter phase where no native session exists yet; gates the start
    /// and clear entry points so two handlers can never race one session slot.</summary>
    private bool busy;
    /// <summary>Every unique seed the current run has delivered, beyond the display cap;
    /// what a concluded run folds into <see cref="target"/> or records in
    /// <see cref="baseRun"/>, so no match is ever lost to the display limit.</summary>
    private readonly List<string> collected = [];
    private readonly HashSet<string> collectedSet = [];
    private bool restoring = true;
    /// <summary>
    /// Anchor for result navigation: the seed of the most recent scout
    /// request, set synchronously so rapid steps chain while a scout is in
    /// flight. A failed request falls back to <see cref="renderedSeed"/>.
    /// </summary>
    private string? scoutedSeed;
    /// <summary>The seed whose manifest the scout pane currently shows.</summary>
    private string? renderedSeed;
    /// <summary>Only the latest scout request may publish its manifest.</summary>
    private int scoutGeneration;
    private bool searchRunning;
    /// <summary>
    /// The query that produced the current results, snapshotted at search
    /// start (or import) so an export never reflects later editor changes.
    /// </summary>
    private QuerySettings? searchedQuery;
    private const int ResultCap = 1024;
    private static readonly string SettingsPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Seed Seeker", "query.json");
    private static readonly string PresetsPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Seed Seeker", "presets.json");
    private static readonly string UpdateStatePath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Seed Seeker", "update.json");
    private bool updateCheckStarted;

    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(nint hwnd);

    public MainWindow()
    {
        InitializeComponent();
        SystemBackdrop = new MicaBackdrop();
        AppWindow.SetIcon(Path.Combine(AppContext.BaseDirectory, "Assets", "SeedSeeker.ico"));
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
        // Decode the item atlases up front so the first sprite render is warm.
        _ = ItemAtlas.GetAsync();
        ResultsList.ItemsSource = results; ScoutButton.IsEnabled = false;
        results.CollectionChanged += (_, _) => UpdateResultNav();
        // J/K step the scout pane through the search results from anywhere in
        // the window except a focused text field.
        if (Content is UIElement root) root.KeyDown += Root_KeyDown;
        // The slider indexes into FloorLimits.Options so empty boss floors (5, 10, 15) are not
        // offered; the converter keeps the thumb tooltip showing the floor, not the raw index.
        FloorSlider.ThumbToolTipValueConverter = new FloorLimitIndexConverter();
        FloorSlider.Minimum = 0; FloorSlider.Maximum = FloorLimits.Options.Length - 1; FloorSlider.Value = 0;
        results.CollectionChanged += (_, _) => UpdateTransferButtons();
        LoadSettings(); LoadPresets(); RefreshPresets(); RefreshQuery(); UpdateTransferButtons();
        Closed += (_, _) => { search?.Cancel(); search?.Dispose(); };
        // ContentDialog needs a live XamlRoot, which only exists once the root
        // element has loaded; Activated can fire before that.
        ((FrameworkElement)Content).Loaded += (_, _) => { if (!updateCheckStarted) { updateCheckStarted = true; _ = CheckForUpdatesAsync(); } };
    }

    private sealed class UpdateState { public string? SkippedVersion { get; set; } public DateTimeOffset LastChecked { get; set; } }

    private sealed record BaseRun(QuerySettings Query, IReadOnlyList<string> Seeds, long ResumeFrom, long Remaining);

    private async Task CheckForUpdatesAsync()
    {
        UpdateState state = new();
        try { if (File.Exists(UpdateStatePath)) state = JsonSerializer.Deserialize<UpdateState>(File.ReadAllText(UpdateStatePath)) ?? new(); } catch { }
        var forced = !string.IsNullOrEmpty(UpdateChecker.FakeLatest);
        if (!forced && DateTimeOffset.UtcNow - state.LastChecked < TimeSpan.FromDays(1)) return;
        state.LastChecked = DateTimeOffset.UtcNow;
        SaveUpdateState(state);
        var version = typeof(MainWindow).Assembly.GetName().Version;
        var current = version is null ? "0.0.0" : $"{version.Major}.{version.Minor}.{version.Build}";
        var update = await UpdateChecker.CheckAsync(current);
        if (update is null || update.Version == state.SkippedVersion) return;
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "Update available",
            Content = $"Seed Seeker {update.Version} is available on GitHub. You have {current}.",
            PrimaryButtonText = "Download",
            SecondaryButtonText = "Skip this version",
            CloseButtonText = "Not now",
            DefaultButton = ContentDialogButton.Primary,
        };
        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
            Process.Start(new ProcessStartInfo(update.Url) { UseShellExecute = true });
        else if (result == ContentDialogResult.Secondary)
        {
            state.SkippedVersion = update.Version;
            SaveUpdateState(state);
        }
    }

    private static void SaveUpdateState(UpdateState state)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(UpdateStatePath)!);
            File.WriteAllText(UpdateStatePath, JsonSerializer.Serialize(state));
        }
        catch { }
    }

    private void LoadSettings()
    {
        restoring = true;
        try { if (File.Exists(SettingsPath)) query = JsonSerializer.Deserialize<QuerySettings>(File.ReadAllText(SettingsPath)) ?? new(); } catch { query = new(); }
        // Settings saved before empty boss floors were removed may hold 5/10/15; snap them below.
        query.MaximumDepth = FloorLimits.Normalize(query.MaximumDepth);
        foreach (var requirement in query.Requirements)
            if (requirement.MaximumDepth is int depth) requirement.MaximumDepth = FloorLimits.Normalize(depth);
        FloorSlider.Value = FloorLimits.IndexOf(query.MaximumDepth); RequireBlacksmith.IsOn = query.RequireBlacksmith; ExcludeRewards.IsOn = query.ExcludeBlacksmithRewards; FastMode.IsOn = query.FastMode; restoring = false;
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
        query.MaximumDepth = FloorLimits.Normalize(query.MaximumDepth);
        foreach (var requirement in query.Requirements)
            if (requirement.MaximumDepth is int depth) requirement.MaximumDepth = FloorLimits.Normalize(depth);
        FloorSlider.Value = FloorLimits.IndexOf(query.MaximumDepth); RequireBlacksmith.IsOn = query.RequireBlacksmith;
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
        FloorLabel.Text = $"first {query.MaximumDepth} floor{(query.MaximumDepth == 1 ? "" : "s")}"; RequireBlacksmith.IsEnabled = query.MaximumDepth < 14; StartButton.IsEnabled = search is not null || (!busy && query.Requirements.Count != 0);
        var count = BitOperations.PopCount((uint)query.Challenges); ChallengeSummary.Text = count == 0 ? "None" : $"{count} enabled";
    }
    private void FloorSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e) { if (restoring || FloorLabel is null) return; query.MaximumDepth = FloorLimits.Options[Math.Clamp((int)e.NewValue, 0, FloorLimits.Options.Length - 1)]; RefreshQuery(); SaveSettings(); }
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
        var maximumUpgrade = r.Kind == ItemKind.Ring ? 4 : 3; var selectedMinimumUpgrade = Math.Clamp(r.Upgrade, 1, maximumUpgrade - 1);
        var upgradeMatch = Combo(["Any", "Exactly", "At least"], (int)r.UpgradeMatch); upgradeMatch.Header = "Upgrade predicate"; var upgrade = Number("Upgrade level", Math.Clamp(r.Upgrade, 1, maximumUpgrade), 1, maximumUpgrade); var upgradeBound = Combo(Enumerable.Range(1, maximumUpgrade - 1).Select(value => $"+{value} or higher"), selectedMinimumUpgrade - 1); upgradeBound.Header = "Minimum upgrade";
        var modifier = new ComboBox { Header = "Enchantment or glyph", HorizontalAlignment = HorizontalAlignment.Stretch };
        var uncursed = new CheckBox { Content = "Require uncursed", IsChecked = r.RequireUncursed };
        var source = Combo(new[] { "Any source" }.Concat(Enum.GetValues<ScoutItemSource>().Select(Labels.Source)), r.Source is null ? 0 : (int)r.Source + 1); source.Header = "Source";
        var group = Combo(["None", "A", "B", "C", "D"], r.IdentityGroup ?? 0); group.Header = "Same-item group";
        var depthToggle = ToggleRow("Limit this item to a floor", r.MaximumDepth is not null, out var depthRow); var depth = Number("Within first floors", FloorLimits.Normalize(r.MaximumDepth ?? 4), 1, 24);
        // Empty boss floors (5, 10, 15) are useless limits: a single upward spin skips to the
        // next real floor, while typed values snap down (10 means the first 10 floors, ≡ 9).
        depth.ValueChanged += (box, args) =>
        {
            if (double.IsNaN(args.NewValue)) return;
            var requested = (int)args.NewValue;
            var previous = double.IsNaN(args.OldValue) ? requested : (int)args.OldValue;
            var target = FloorLimits.SkipTarget(previous, requested);
            if (target != requested) box.Value = target;
        };
        var content = new StackPanel { Spacing = 12, Padding = new Thickness(2, 4, 2, 4) }; foreach (var control in new UIElement[] { kind, item, tierMatch, tier, tierBound, upgradeMatch, upgrade, upgradeBound, modifier, uncursed, source, group, depthRow, depth }) content.Children.Add(control);
        void NormalizeTier()
        {
            var predicate = (TierMatch)Math.Max(0, tierMatch.SelectedIndex);
            selectedTier = predicate is TierMatch.AtLeast or TierMatch.AtMost ? Math.Clamp(selectedTier, 3, 4) : Math.Clamp(selectedTier, 2, 5);
            tier.Value = selectedTier; tierBound.SelectedIndex = Math.Clamp(selectedTier, 3, 4) - 3;
        }
        void SyncVisibility()
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex); var generic = item.SelectedIndex == 0 && k.Family() is ItemKind.Weapon or ItemKind.Armor;
            var predicate = (TierMatch)Math.Max(0, tierMatch.SelectedIndex); var ranged = predicate is TierMatch.AtLeast or TierMatch.AtMost;
            tierMatch.Visibility = generic ? Visibility.Visible : Visibility.Collapsed;
            tier.Visibility = generic && predicate == TierMatch.Exactly ? Visibility.Visible : Visibility.Collapsed;
            tierBound.Visibility = generic && ranged ? Visibility.Visible : Visibility.Collapsed;
            tierBound.Header = predicate == TierMatch.AtLeast ? "Minimum tier" : "Maximum tier";
            var upgradePredicate = (UpgradeMatch)Math.Max(0, upgradeMatch.SelectedIndex); var ringMinimum = k == ItemKind.Ring && upgradePredicate == UpgradeMatch.AtLeast;
            upgrade.Visibility = upgradePredicate == UpgradeMatch.Exactly || ringMinimum ? Visibility.Visible : Visibility.Collapsed;
            upgrade.Header = ringMinimum ? "Minimum upgrade" : "Upgrade level";
            upgradeBound.Visibility = upgradePredicate == UpgradeMatch.AtLeast && !ringMinimum ? Visibility.Visible : Visibility.Collapsed;
        }
        void NormalizeUpgrade()
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex); maximumUpgrade = k == ItemKind.Ring ? 4 : 3;
            var atLeast = upgradeMatch.SelectedIndex == (int)UpgradeMatch.AtLeast;
            upgrade.Maximum = atLeast ? maximumUpgrade - 1 : maximumUpgrade;
            upgrade.Value = Math.Clamp(double.IsNaN(upgrade.Value) ? 1 : upgrade.Value, 1, upgrade.Maximum);
        }
        void PopulateModifiers(string? selection)
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex);
            var modifiers = ItemCatalog.Modifiers(k)
                .Where(effect => uncursed.IsChecked != true || !ItemCatalog.IsCurse(k, effect))
                .ToList();
            modifier.Items.Clear(); modifier.Items.Add("None"); foreach (var value in modifiers) modifier.Items.Add(value);
            modifier.SelectedIndex = selection is null ? 0 : Math.Max(0, modifiers.IndexOf(selection) + 1);
            modifier.Visibility = k.Family() is ItemKind.Weapon or ItemKind.Armor ? Visibility.Visible : Visibility.Collapsed;
        }
        void Populate()
        {
            var k = (ItemKind)Math.Max(0, kind.SelectedIndex); var oldId = r.Item?.Id; var items = ItemCatalog.For(k).ToList(); item.Items.Clear(); item.Items.Add($"Any {Labels.Singular(k)}"); foreach (var value in items) item.Items.Add(value.Name); item.SelectedIndex = Math.Max(0, items.FindIndex(x => x.Id == oldId) + 1);
            PopulateModifiers(r.Modifier);
            maximumUpgrade = k == ItemKind.Ring ? 4 : 3; NormalizeUpgrade();
            selectedMinimumUpgrade = Math.Clamp(selectedMinimumUpgrade, 1, maximumUpgrade - 1); upgradeBound.Items.Clear(); foreach (var value in Enumerable.Range(1, maximumUpgrade - 1)) upgradeBound.Items.Add($"+{value} or higher"); upgradeBound.SelectedIndex = selectedMinimumUpgrade - 1; SyncVisibility();
        }
        kind.SelectionChanged += (_, _) => { r.Item = null; r.Modifier = null; Populate(); }; item.SelectionChanged += (_, _) => SyncVisibility(); tier.ValueChanged += (_, _) => { if (!double.IsNaN(tier.Value)) selectedTier = (int)tier.Value; }; tierBound.SelectionChanged += (_, _) => { if (tierBound.SelectedIndex >= 0) selectedTier = tierBound.SelectedIndex + 3; }; tierMatch.SelectionChanged += (_, _) => { NormalizeTier(); SyncVisibility(); }; upgradeMatch.SelectionChanged += (_, _) => { NormalizeUpgrade(); SyncVisibility(); }; upgradeBound.SelectionChanged += (_, _) => { if (upgradeBound.SelectedIndex >= 0) selectedMinimumUpgrade = upgradeBound.SelectedIndex + 1; }; uncursed.Checked += (_, _) => PopulateModifiers(modifier.SelectedItem is string effect && !ItemCatalog.IsCurse((ItemKind)Math.Max(0, kind.SelectedIndex), effect) ? effect : null); uncursed.Unchecked += (_, _) => PopulateModifiers(modifier.SelectedItem?.ToString()); depthToggle.Toggled += (_, _) => depth.Visibility = depthToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
        Populate(); NormalizeTier(); SyncVisibility(); depth.Visibility = depthToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = isNew ? "New Requirement" : "Edit Requirement", PrimaryButtonText = isNew ? "Add" : "Save", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Primary, Content = VerticalScrollView(content, 510, 430) };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return false;
        r.Kind = (ItemKind)kind.SelectedIndex; r.Item = item.SelectedIndex > 0 ? ItemCatalog.For(r.Kind).ElementAt(item.SelectedIndex - 1) : null; r.TierMatch = r.Item is null && r.Kind.Family() is ItemKind.Weapon or ItemKind.Armor ? (TierMatch)tierMatch.SelectedIndex : TierMatch.Any; r.Tier = r.TierMatch == TierMatch.Any ? 0 : selectedTier;
        r.UpgradeMatch = (UpgradeMatch)upgradeMatch.SelectedIndex; r.Upgrade = r.UpgradeMatch switch { UpgradeMatch.Any => 0, UpgradeMatch.Exactly => (int)upgrade.Value, UpgradeMatch.AtLeast when r.Kind == ItemKind.Ring => (int)upgrade.Value, UpgradeMatch.AtLeast => selectedMinimumUpgrade, _ => 0 }; r.Modifier = modifier.Visibility == Visibility.Visible && modifier.SelectedIndex > 0 ? modifier.SelectedItem?.ToString() : null;
        r.RequireUncursed = uncursed.IsChecked == true; r.Source = source.SelectedIndex == 0 ? null : (ScoutItemSource)(source.SelectedIndex - 1); r.IdentityGroup = group.SelectedIndex == 0 ? null : group.SelectedIndex; r.MaximumDepth = depthToggle.IsOn ? FloorLimits.Normalize(Math.Clamp((int)depth.Value, 1, 24)) : null; return true;
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
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Challenges", PrimaryButtonText = "Done", DefaultButton = ContentDialogButton.Primary, Content = VerticalScrollView(panel, 520) };
        await dialog.ShowAsync(); query.Challenges = toggles.Where(x => x.Item2.IsOn).Aggregate(0, (mask, x) => mask | x.Item1); RefreshQuery(); SaveSettings();
    }

    /// <summary>
    /// Attribution facts for the bundled Shattered Pixel Dungeon artwork, matching
    /// the Android About screen so both platforms state the same thing.
    /// </summary>
    private static readonly (string Label, string Value)[] ArtworkAttribution =
    [
        ("Pixel Dungeon", "© 2012–2015 Oleg Dolya / Watabou"),
        ("Shattered Pixel Dungeon", "© 2014–2026 Evan Debenham"),
        ("Upstream", "Shattered Pixel Dungeon v3.3.8"),
        ("Commit", "7b8b845a76fe76c6b7c031ae9e570852411f56db"),
        ("Atlas SHA-256", "ce2496368660e9b2…a294caacaf"),
        ("Icon SHA-256", "38df728d32842d9f…24d7eb9b72"),
    ];

    private static Brush ThemeBrush(string key, Color fallback)
    {
        try { return (Brush)Application.Current.Resources[key]; }
        catch { return new SolidColorBrush(fallback); }
    }

    /// <summary>
    /// The app ships GPL-3.0-or-later artwork from Shattered Pixel Dungeon, so it
    /// has to surface the attribution and a way to read the full license text.
    ///
    /// Every passage below is quoted verbatim from README.md, minus its inline link
    /// markup, and matches the Android About screen line for line. Keep it that way:
    /// the app's prose is the project's own, not a second description of it that can
    /// drift. Section titles are the README's own headings.
    /// </summary>
    private async void About_Click(object sender, RoutedEventArgs e)
    {
        var secondary = ThemeBrush("TextFillColorSecondaryBrush", Microsoft.UI.Colors.Gray);
        var accent = ThemeBrush("AccentTextFillColorPrimaryBrush", Microsoft.UI.Colors.SteelBlue);
        var version = typeof(MainWindow).Assembly.GetName().Version;
        var current = version is null ? "0.0.0" : $"{version.Major}.{version.Minor}.{version.Build}";
        var panel = new StackPanel { Spacing = 12, Width = 460 };

        var heading = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 16 };
        heading.Children.Add(new Image { Width = 68, Height = 68, Source = await BrandMarkAsync(), VerticalAlignment = VerticalAlignment.Center });
        heading.Children.Add(new TextBlock { Text = "Seed Seeker", FontSize = 24, FontWeight = FontWeights.SemiBold, VerticalAlignment = VerticalAlignment.Center });
        panel.Children.Add(heading);

        // The README's opening line sits under its "# Seed Seeker" heading, so it
        // reads as a lede here rather than as a card that would repeat the title
        // above it.
        panel.Children.Add(new TextBlock
        {
            Text = "An extremely fast seed finder for Shattered Pixel Dungeon, written in Rust — with native apps for Android, Linux, macOS, and Windows.",
            TextWrapping = TextWrapping.Wrap,
            Foreground = secondary,
            Margin = new Thickness(4, 0, 4, 0),
        });

        var acknowledgements = AboutSection(panel, "Acknowledgements");
        acknowledgements.Children.Add(AboutText("Seed Seeker reimplements the generation of Shattered Pixel Dungeon by Evan Debenham, itself based on Pixel Dungeon by Oleg Dolya."));
        acknowledgements.Children.Add(AboutText("Elektrochecker's shpd-seed-finder serves as an oracle for this project's parity tests."));

        var license = AboutSection(panel, "License and identity");
        license.Children.Add(AboutText("This project is GPL-3.0-or-later. It contains a derived generation implementation and an unchanged item sprite atlas from Shattered Pixel Dungeon."));
        var attribution = new StackPanel { Spacing = 8 };
        foreach (var (label, value) in ArtworkAttribution)
        {
            var line = new StackPanel { Spacing = 1 };
            line.Children.Add(new TextBlock { Text = label, FontSize = 11, Foreground = accent });
            line.Children.Add(new TextBlock { Text = value, FontSize = 12, Foreground = secondary, TextWrapping = TextWrapping.Wrap });
            attribution.Children.Add(line);
        }
        license.Children.Add(attribution);
        license.Children.Add(FileReader("LICENSE.txt", "Read full license"));

        panel.Children.Add(new TextBlock
        {
            Text = $"Seed Seeker {current} · Shattered Pixel Dungeon v3.3.8 profile",
            FontSize = 11,
            Foreground = secondary,
            TextWrapping = TextWrapping.Wrap,
            TextAlignment = TextAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Center,
            Margin = new Thickness(0, 2, 0, 2),
        });

        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot,
            Title = "About & licenses",
            CloseButtonText = "Close",
            DefaultButton = ContentDialogButton.Close,
            Content = VerticalScrollView(panel, 520, 500),
        };
        await dialog.ShowAsync();
    }

    /// <summary>
    /// A titled card appended to <paramref name="parent"/>, mirroring the Android
    /// About screen's sections; the returned panel holds the section's body.
    /// </summary>
    private static StackPanel AboutSection(StackPanel parent, string title)
    {
        var body = new StackPanel { Spacing = 10 };
        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(new TextBlock { Text = title, FontWeight = FontWeights.SemiBold });
        content.Children.Add(body);
        parent.Children.Add(new Border
        {
            Style = (Style)Application.Current.Resources["SettingsCard"],
            Padding = new Thickness(16),
            Child = content,
        });
        return body;
    }

    private static TextBlock AboutText(string text) => new() { Text = text, TextWrapping = TextWrapping.Wrap };

    /// <summary>
    /// The app icon, for the About dialog's brand mark. The bundled .ico stores its
    /// frames smallest first, so the largest one is picked explicitly rather than
    /// letting the decoder settle for the 16×16 frame.
    /// </summary>
    private static async Task<ImageSource?> BrandMarkAsync()
    {
        try
        {
            var file = await StorageFile.GetFileFromPathAsync(Path.Combine(AppContext.BaseDirectory, "Assets", "SeedSeeker.ico"));
            using var stream = await file.OpenReadAsync();
            var decoder = await BitmapDecoder.CreateAsync(stream);
            var frame = await decoder.GetFrameAsync(0);
            for (uint index = 1; index < decoder.FrameCount; index++)
            {
                var candidate = await decoder.GetFrameAsync(index);
                if (candidate.PixelWidth > frame.PixelWidth) frame = candidate;
            }
            var data = await frame.GetPixelDataAsync(
                BitmapPixelFormat.Bgra8,
                BitmapAlphaMode.Premultiplied,
                new BitmapTransform(),
                ExifOrientationMode.IgnoreExifOrientation,
                ColorManagementMode.DoNotColorManage);
            var bitmap = new WriteableBitmap((int)frame.PixelWidth, (int)frame.PixelHeight);
            WindowsRuntimeBufferExtensions.CopyTo(data.DetachPixelData(), bitmap.PixelBuffer);
            bitmap.Invalidate();
            return bitmap;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>An expander that reads a bundled text file the first time it opens.</summary>
    private static Expander FileReader(string name, string header)
    {
        var expander = new Expander
        {
            Header = header,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            Margin = new Thickness(0, 8, 0, 0),
        };
        expander.Expanding += (_, _) => expander.Content ??= BundledText(name);
        return expander;
    }

    private static TextBlock BundledText(string name)
    {
        string text;
        try { text = File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Assets", name)); }
        catch (Exception ex) { text = $"{name} could not be read: {ex.Message}"; }
        return new TextBlock
        {
            Text = text,
            FontFamily = new FontFamily("Cascadia Mono, Consolas"),
            FontSize = 11,
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true,
        };
    }

    private static ScrollView VerticalScrollView(UIElement content, double maxHeight, double? width = null)
    {
        var scrollView = new ScrollView
        {
            Content = content,
            HorizontalScrollMode = ScrollingScrollMode.Disabled,
            HorizontalScrollBarVisibility = ScrollingScrollBarVisibility.Hidden,
            MaxHeight = maxHeight,
        };
        if (width is double value) scrollView.Width = value;
        return scrollView;
    }

    private async void Start_Click(object sender, RoutedEventArgs e)
    {
        if (search is not null) { search.Cancel(); StartButton.IsEnabled = false; return; }
        if (busy) return;
        // Start is the single entry point: the query's relationship to the
        // session's Target decides what happens (docs/search-semantics.md).
        // A continuation refines the Target Set and resumes its coverage, a
        // query sharing an item filters the full set, and anything else scans
        // the whole range without touching the Target — continuing the
        // previous detached scan when that is sound. None of this is a user
        // decision; only Clear Results discards anything.
        switch (SearchPlan.DecideStart(query, target, lastRunDetached ? baseRun?.Query : null))
        {
            case StartMode.TargetRefine: await RefineTarget(target!, resume: true); return;
            case StartMode.TargetFilter: await RefineTarget(target!, resume: false); return;
            case StartMode.ContinueDetached: await RefineSearch(baseRun!); return;
            case StartMode.Detached: await StartScan(detached: true); return;
            default: await StartScan(detached: false); return;
        }
    }
    /// <summary>Status-bar notice for a fresh detached scan, shown while the
    /// display and the Target Set diverge; cleared at the usual clear points.</summary>
    private const string UnrelatedNotice = "Unrelated query — detached search from previous results.";
    /// <summary>
    /// Scans the full seed range from scratch, replacing the displayed
    /// results. An anchor scan establishes the Target when it concludes; a
    /// detached scan leaves the existing Target untouched for later related
    /// searches and announces that in the status bar.
    /// </summary>
    private async Task StartScan(bool detached)
    {
        busy = true; collected.Clear(); collectedSet.Clear(); results.Clear(); SearchStatus.Text = "Starting search…";
        var notice = detached ? UnrelatedNotice : null;
        SetStatusBar(notice); SetStartButton(running: true);
        try
        {
            var snapshot = query.Clone();
            // Snapshot the query so an export always describes the query that
            // actually produced the listed results, even after later edits.
            searchedQuery = snapshot;
            search = await Task.Run(() => engine.Start(snapshot)); await RunSearch(search, notice); await CaptureBaseRun(snapshot, search, detached ? RunKind.Detached : RunKind.Anchor);
        }
        catch (Exception ex) { SearchStatus.Text = $"Failed: {ex.Message}"; baseRun = null; lastRunDetached = false; }
        finally { busy = false; search?.Dispose(); search = null; SetStartButton(running: false); StartButton.IsEnabled = query.Requirements.Count != 0; }
    }
    /// <summary>
    /// Refines against the Target: the full Target Set is re-verified through
    /// the current query and the survivors become the displayed results; in
    /// resume mode only, the scan then picks up the target's uncovered
    /// remainder, whose new finds join the Target Set as its coverage
    /// advances. The base is always the full Target Set rather than the last
    /// run's survivors, so loosening back toward the Target Query brings
    /// previously dropped seeds back. A failure leaves the previous display
    /// and the Target fully intact. Only <see cref="Start_Click"/> calls this,
    /// after its own re-entry guards, so the session slot is never contested.
    /// </summary>
    private async Task RefineTarget(TargetRun anchor, bool resume)
    {
        busy = true;
        var snapshot = query.Clone(); SetStatusBar("Verifying previous results…"); SetStartButton(running: true); StartButton.IsEnabled = false;
        try
        {
            // Filter before touching the displayed results, so a failure here
            // leaves the previous run's display fully intact.
            var kept = await Task.Run(() => engine.FilterSeeds(snapshot, anchor.Seeds));
            results.Clear(); collected.Clear(); collectedSet.Clear();
            Collect(kept);
            // From here on the listed results match the refined query, so
            // that is what an export must claim. A failure above leaves the
            // previous results — and their snapshot — untouched.
            searchedQuery = snapshot;
            // This run belongs to the target thread, so a later unrelated
            // query may no longer continue an older detached scan.
            lastRunDetached = false;
            var summary = $"Refined: kept {kept.Count} of {anchor.Seeds.Count} previous seed{(anchor.Seeds.Count == 1 ? "" : "s")}";
            if (resume && anchor.Remaining > 0)
            {
                // Always resume, even when the survivors already fill the
                // display: the engine accepts up to another cap's worth of new
                // finds per session, and every one of them joins the uncapped
                // Target Set through `collected` whether or not it can be
                // listed. Repeating an identical query therefore keeps growing
                // the Target Set by roughly a cap per run.
                SetStatusBar($"{summary} — searching for more…");
                search = await Task.Run(() => engine.StartResumed(snapshot, anchor.ResumeFrom, anchor.Remaining));
                StartButton.IsEnabled = true;
                await RunSearch(search, summary); await CaptureBaseRun(snapshot, search, RunKind.TargetRefine);
            }
            // A filter-only run (or a refine with nothing left to scan) scans
            // nothing: the Target Set and its coverage stay exactly as they were.
            else { SearchStatus.Text = "Completed"; SetStatusBar(results.Count >= ResultCap ? WithCapNotice(summary) : summary); baseRun = new(snapshot, [.. collected], anchor.ResumeFrom, 0); }
        }
        // The Target stays valid on failure: nothing of its coverage was
        // consumed, so the refine can simply be retried.
        catch (Exception ex) { SearchStatus.Text = $"Refine failed: {ex.Message}"; SetStatusBar(null); }
        finally { busy = false; search?.Dispose(); search = null; SetStartButton(running: false); StartButton.IsEnabled = query.Requirements.Count != 0; }
    }
    /// <summary>
    /// Continues the previous detached scan (the classic pre-Target refine
    /// behaviour, scoped to the detached thread): its delivered seeds are
    /// filtered through the current query, then the scan resumes where that
    /// run stopped. The query may equal the run's — the filter then keeps
    /// everything and this is a plain "continue". The Target is untouched
    /// throughout. Only <see cref="Start_Click"/> calls this, after its own
    /// re-entry guards, so the session slot is never contested.
    /// </summary>
    private async Task RefineSearch(BaseRun previous)
    {
        busy = true;
        var snapshot = query.Clone(); SetStatusBar("Verifying previous results…"); SetStartButton(running: true); StartButton.IsEnabled = false;
        try
        {
            // Filter before touching the displayed results, so a failure here
            // leaves the previous run (and its refinable base) fully intact.
            var kept = await Task.Run(() => engine.FilterSeeds(snapshot, previous.Seeds));
            results.Clear(); collected.Clear(); collectedSet.Clear();
            Collect(kept);
            // From here on the listed results match the refined query, so
            // that is what an export must claim. A failure above leaves the
            // previous results — and their snapshot — untouched.
            searchedQuery = snapshot;
            var summary = $"Refined: kept {kept.Count} of {previous.Seeds.Count} previous seed{(previous.Seeds.Count == 1 ? "" : "s")}";
            if (previous.Remaining > 0)
            {
                // Always resume, even when the survivors already fill the
                // display: new finds beyond the cap still enter `collected`,
                // the continuation base a later refine filters.
                SetStatusBar($"{summary} — searching for more…");
                search = await Task.Run(() => engine.StartResumed(snapshot, previous.ResumeFrom, previous.Remaining));
                StartButton.IsEnabled = true;
                await RunSearch(search, summary); await CaptureBaseRun(snapshot, search, RunKind.Detached);
            }
            else { SearchStatus.Text = "Completed"; SetStatusBar(results.Count >= ResultCap ? WithCapNotice(summary) : summary); baseRun = new(snapshot, [.. collected], previous.ResumeFrom, 0); }
        }
        // The previous base run stays valid on failure: nothing of its
        // coverage was consumed, so the refine can simply be retried.
        catch (Exception ex) { SearchStatus.Text = $"Refine failed: {ex.Message}"; SetStatusBar(null); }
        finally { busy = false; search?.Dispose(); search = null; SetStartButton(running: false); StartButton.IsEnabled = query.Requirements.Count != 0; }
    }
    /// <summary>
    /// Records every unique delivered seed; the visible list is capped while
    /// the full set stays available as a later refine's filter input.
    /// </summary>
    private void Collect(IEnumerable<string> seeds)
    {
        foreach (var seed in seeds)
        {
            if (!collectedSet.Add(seed)) continue;
            collected.Add(seed);
            if (results.Count < ResultCap) results.Add(new(seed, results.Count + 1));
        }
    }
    /// <summary>How the run being settled relates to the Target, for <see cref="CaptureBaseRun"/>.</summary>
    private enum RunKind { Anchor, TargetRefine, Detached }
    /// <summary>
    /// Settles a run that just ended, recording it in <see cref="baseRun"/>:
    /// the query as it ran, every delivered seed (not just the displayed
    /// ones), and where a resumed scan must pick up. Called after the poll
    /// loop ends but before the session is disposed, since the hint is only
    /// exact once the session has stopped. The engine keeps reporting Running
    /// until its queue is drained, so a terminal status implies nothing is
    /// left undelivered. Per docs/search-semantics.md, an anchor run
    /// establishes the Target from its own results and coverage, a target
    /// refine grows the Target Set with its new finds and advances its
    /// coverage, and a detached run leaves the Target exactly as it was. A
    /// failed run establishes nothing — its coverage is unknown — and never
    /// touches the Target.
    /// </summary>
    private async Task CaptureBaseRun(QuerySettings ranQuery, NativeSearch active, RunKind kind)
    {
        var status = await Task.Run(active.Status);
        if (status.State == SearchState.Failed) { baseRun = null; lastRunDetached = false; return; }
        var (resumeFrom, remaining) = await Task.Run(active.ResumeHint);
        baseRun = new(ranQuery, [.. collected], resumeFrom, remaining);
        lastRunDetached = kind == RunKind.Detached;
        if (kind == RunKind.Anchor)
            target = new(ranQuery, [.. collected], resumeFrom, remaining);
        else if (kind == RunKind.TargetRefine && target is TargetRun anchor)
        {
            // The refined run's survivors were already members; only new finds
            // from the resumed scan grow the set, which is never capped. The
            // Target Query stays fixed — the finds match it by construction.
            var seeds = new List<string>(anchor.Seeds); var seen = new HashSet<string>(anchor.Seeds);
            foreach (var seed in collected) if (seen.Add(seed)) seeds.Add(seed);
            target = new(anchor.Query, seeds, resumeFrom, remaining);
        }
    }
    /// <summary>
    /// Writes the window-bottom status bar, the sole home of the transient
    /// refine-progress, refined-summary, and result-cap notices; null clears
    /// the text while the bar itself stays put, so the layout never jumps.
    /// </summary>
    private void SetStatusBar(string? text) => StatusBarText.Text = text ?? "";
    /// <summary>Display-truncation notice for the status bar, joined to a run's
    /// summary when one exists. It reports that the *listing* stopped at
    /// <see cref="ResultCap"/> rows — every further find still reaches
    /// <see cref="collected"/> and the Target Set.</summary>
    private static string WithCapNotice(string? summary) => summary is null
        ? "Result limit reached (1,024 seeds)."
        : $"{summary} · Result limit reached (1,024 seeds).";
    private void SetStartButton(bool running)
    {
        StartIcon.Glyph = running ? "" : "";
        StartLabel.Text = running ? "Cancel Search" : "Start Search";
        PresetPicker.IsEnabled = !running;
        SavePresetButton.IsEnabled = !running;
        DeletePresetButton.IsEnabled = !running
            && PresetPicker.SelectedItem is QueryPreset { IsBuiltIn: false };
        searchRunning = running;
        UpdateTransferButtons();
    }

    private void UpdateTransferButtons()
    {
        ImportResultsButton.IsEnabled = !searchRunning;
        ExportResultsButton.IsEnabled = !searchRunning && results.Count > 0 && searchedQuery is not null;
        // `busy` also covers a refine's filter phase, which owns the results
        // even though no native session exists yet.
        ClearResultsButton.IsEnabled = !searchRunning && !busy
            && (results.Count > 0 || collected.Count > 0 || baseRun is not null || target is not null);
    }

    /// <summary>
    /// Returns the results area to its idle state, dropping the Target — its
    /// query, seed set, and coverage — along with the last run's record, so the
    /// next search anchors a new session from scratch. This is the only way to
    /// end a session: every other path keeps it alive.
    /// </summary>
    private void ClearResults_Click(object sender, RoutedEventArgs e)
    {
        if (busy || search is not null) return;
        results.Clear(); collected.Clear(); collectedSet.Clear();
        baseRun = null; target = null; lastRunDetached = false; searchedQuery = null;
        SearchStatus.Text = "Add requirements, then press Start Search."; SetStatusBar(null);
        UpdateTransferButtons();
    }

    private async void ExportResults_Click(object sender, RoutedEventArgs e)
    {
        // Export the query snapshot captured when the results were produced
        // (at search start or import), never the live editor state.
        // `busy` also covers a refine's filter phase, which runs with no
        // native session but replaces the results when it lands.
        if (busy || search is not null || searchedQuery is null || results.Count == 0) return;
        var exportQuery = searchedQuery.Clone();
        var seeds = results.Select(x => x.Seed).ToList();
        var picker = new Windows.Storage.Pickers.FileSavePicker
        {
            SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.DocumentsLibrary,
            SuggestedFileName = ResultsExport.SuggestedFileName,
        };
        picker.FileTypeChoices.Add("Seed Seeker results", [".json"]);
        // Unpackaged apps must bind pickers to the window handle before use.
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var file = await picker.PickSaveFileAsync();
        if (file is null) return;
        try
        {
            var version = typeof(MainWindow).Assembly.GetName().Version;
            var appVersion = version is null ? "dev" : $"{version.Major}.{version.Minor}.{version.Build}";
            var contents = ResultsExport.Encode(exportQuery, seeds, appVersion);
            await FileIO.WriteTextAsync(file, contents);
        }
        catch (Exception ex)
        {
            await ShowTransferMessage($"Export failed: {ex.Message}");
        }
    }

    private async void ImportResults_Click(object sender, RoutedEventArgs e)
    {
        if (busy || search is not null) return;
        var picker = new Windows.Storage.Pickers.FileOpenPicker
        {
            SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.DocumentsLibrary,
        };
        picker.FileTypeFilter.Add(".json");
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is null) return;
        try
        {
            var properties = await file.GetBasicPropertiesAsync();
            if (properties.Size > ResultsExport.MaxFileBytes)
            {
                await ShowTransferMessage("This file is too large to be a Seed Seeker results file (2 MiB limit).");
                return;
            }
            var text = await FileIO.ReadTextAsync(file);
            // Parse the untrusted file off the UI thread.
            var imported = await Task.Run(() => ResultsExport.Decode(text));
            // A search or refine may have started while the picker or reads
            // were pending.
            if (busy || search is not null)
            {
                await ShowTransferMessage("Stop the search before importing results.");
                return;
            }
            ApplyQuery(imported.Query);
            var snapshot = imported.Query.Clone();
            searchedQuery = snapshot;
            // Imported results carry no traversal state, so the previous
            // search's record — and the seeds collected as its filter input —
            // no longer describe the listed seeds.
            baseRun = null; lastRunDetached = false;
            results.Clear(); collected.Clear(); collectedSet.Clear(); SetStatusBar(null);
            // Deduplicate then cap, the shared import rule on every platform.
            foreach (var seed in imported.Seeds.Distinct())
                if (results.Count < ResultCap) results.Add(new(seed, results.Count + 1));
            // The imported query and seeds become the session's Target, with
            // no coverage: refines of an import are filter-only.
            target = new(snapshot, results.Select(x => x.Seed).ToList(), 0, 0);
            var dropped = imported.Seeds.Count - results.Count;
            var status = $"Imported {results.Count} seed{(results.Count == 1 ? "" : "s")} from file.";
            if (dropped > 0)
                status += $"\n{dropped} duplicate or over-limit entr{(dropped == 1 ? "y" : "ies")} dropped.";
            if (imported.FileShpdVersion is string fileVersion && fileVersion != ResultsExport.ShpdVersion)
                status += $"\nMade for Shattered Pixel Dungeon v{fileVersion}; this app targets v{ResultsExport.ShpdVersion}, so seeds may generate differently.";
            SearchStatus.Text = status;
            UpdateTransferButtons();
        }
        catch (ResultsExportException ex)
        {
            await ShowTransferMessage(ex.Message);
        }
        catch (Exception ex)
        {
            await ShowTransferMessage($"Import failed: {ex.Message}");
        }
    }

    private async Task ShowTransferMessage(string message)
    {
        var dialog = new ContentDialog { XamlRoot = Content.XamlRoot, Title = "Results file", Content = message, CloseButtonText = "OK" };
        await dialog.ShowAsync();
    }

    /// <summary>
    /// Polls the running session into <see cref="SearchStatus"/>. A resumed
    /// refine passes its summary so the status bar keeps reporting the refine
    /// while the scan runs, and so the result-cap notice can join it.
    /// </summary>
    private async Task RunSearch(NativeSearch active, string? summary = null)
    {
        var timer = Stopwatch.StartNew(); long lastScanned = 0; var lastTime = 0d;
        while (true)
        {
            await Task.Delay(150); var batch = await Task.Run(() => active.Poll(128)); Collect(batch);
            var status = await Task.Run(active.Status); var seconds = timer.Elapsed.TotalSeconds; var rate = seconds > lastTime ? (status.Scanned - lastScanned) / (seconds - lastTime) : 0; lastScanned = status.Scanned; lastTime = seconds;
            var probability = status.Probability > 0 ? $"{status.Probability:P4}" : "calculating"; var tts = status.Probability > 0 && rate > 0 ? FormatDuration(1 / status.Probability / rate) : "calculating";
            SearchStatus.Text = status.State == SearchState.Running ? $"Seed match probability: {probability}   •   TTS @ {rate:N0} seeds/s: {tts}\nTime elapsed: {FormatDuration(seconds)}   •   Seeds searched: {status.Scanned:N0}" : status.State switch { SearchState.Completed => "Completed", SearchState.Cancelled => "Cancelled", _ => $"Failed (error {status.ErrorCode})" };
            // The engine reports a terminal state only once every queued match
            // has been drained, so breaking here never leaves seeds behind —
            // including a session that stopped itself at its accept cap.
            if (status.State != SearchState.Running) break;
        }
        // Only the concluded run announces the cap: a full display during an
        // accumulating scan is the expected state ("searching for more…" says
        // what is happening), and every further find still reached
        // `collected` and the Target.
        SetStatusBar(results.Count >= ResultCap ? WithCapNotice(summary) : summary);
    }
    private static string FormatDuration(double seconds) => seconds switch { < 1 => "less than a second", < 60 => $"{seconds:N0}s", < 3600 => $"{seconds / 60:N1}m", < 86400 => $"{seconds / 3600:N1}h", _ => $"{seconds / 86400:N1}d" };

    private void ResultsList_SelectionChanged(object sender, SelectionChangedEventArgs e) { if (ResultsList.SelectedItem is SeedResult row) { SeedInput.Text = row.Seed; _ = ScoutSeed(row.Seed); } }
    private void ResultsList_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e) { if (ResultsList.SelectedItem is SeedResult row) Copy(row.Seed); }
    private void CopyResult_Click(object sender, RoutedEventArgs e) { if ((sender as FrameworkElement)?.DataContext is SeedResult row) Copy(row.Seed); }
    private void ScoutResult_Click(object sender, RoutedEventArgs e) { if ((sender as FrameworkElement)?.DataContext is SeedResult row) { SeedInput.Text = row.Seed; _ = ScoutSeed(row.Seed); } }
    private void SeedInput_TextChanged(object sender, TextChangedEventArgs e) { var formatted = SeedCode.Format(SeedInput.Text); if (formatted != SeedInput.Text) { SeedInput.Text = formatted; SeedInput.SelectionStart = formatted.Length; } ScoutButton.IsEnabled = SeedCode.IsCanonical(formatted); }
    private void SeedInput_KeyDown(object sender, KeyRoutedEventArgs e) { if (e.Key == VirtualKey.Enter && SeedCode.IsCanonical(SeedInput.Text)) { _ = ScoutSeed(SeedInput.Text); e.Handled = true; } }
    private async void Scout_Click(object sender, RoutedEventArgs e) => await ScoutSeed(SeedInput.Text);
    private List<string> ResultSeeds() => results.Select(result => result.Seed).ToList();
    /// <summary>Steps the scouted seed through the search results; returns false (inert) when the scouted seed is not one of them or the step cannot move.</summary>
    private bool NavigateResult(int delta)
    {
        if (ResultNavigation.Step(ResultSeeds(), scoutedSeed, delta) is not int target) return false;
        // The selection-changed handler fills the seed field and scouts.
        ResultsList.SelectedIndex = target;
        ResultsList.ScrollIntoView(results[target]);
        return true;
    }
    private void UpdateResultNav()
    {
        if (ResultNavigation.IndexOf(ResultSeeds(), scoutedSeed) is not int index) { ResultNav.Visibility = Visibility.Collapsed; return; }
        ResultNav.Visibility = Visibility.Visible;
        ResultPosition.Text = $"Result {index + 1} of {results.Count}";
        PrevResultButton.IsEnabled = index > 0;
        NextResultButton.IsEnabled = index < results.Count - 1;
    }
    private void PrevResult_Click(object sender, RoutedEventArgs e) => NavigateResult(-1);
    private void NextResult_Click(object sender, RoutedEventArgs e) => NavigateResult(1);
    private void Root_KeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key is not (VirtualKey.J or VirtualKey.K)) return;
        // Never steal letters from a focused text input.
        if (e.OriginalSource is TextBox or NumberBox or AutoSuggestBox or PasswordBox) return;
        if (IsKeyDown(VirtualKey.Control) || IsKeyDown(VirtualKey.Menu)) return;
        // Only swallow the key when navigation actually moved: an inert j/k
        // must stay available to list type-ahead and combo type-select.
        e.Handled = NavigateResult(e.Key == VirtualKey.J ? 1 : -1);
    }
    private static bool IsKeyDown(VirtualKey key) =>
        Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(key)
            .HasFlag(Windows.UI.Core.CoreVirtualKeyStates.Down);

    private async Task ScoutSeed(string seed)
    {
        var generation = ++scoutGeneration;
        scoutedSeed = seed; UpdateResultNav();
        ScoutButton.IsEnabled = false; ScoutStatus.Text = "Scouting…";
        try
        {
            var world = await Task.Run(() => engine.Scout(seed, query.Challenges));
            if (generation != scoutGeneration) return;
            var matches = ScoutMatcher.SelectMatches(world.Items, query.Requirements,
                query.MaximumDepth, query.ExcludeBlacksmithRewards);
            var groups = world.Items.Select((item, index) => (Item: item, Index: index))
                .GroupBy(x => x.Item.Depth).OrderBy(g => g.Key).Select(g =>
            {
                var group = new ScoutGroup { Floor = $"Floor {g.Key}", Region = Region(g.Key), Quest = QuestLabel(world.Quests, g.Key) };
                group.AddRange(g.Select(entry => ScoutRow.From(entry.Item, matches.Contains(entry.Index)))); return group;
            }).ToList();
            ScoutList.ItemsSource = new CollectionViewSource { IsSourceGrouped = true, Source = groups }.View;
            QuestStrip.Children.Clear();
            foreach (var quest in world.Quests) QuestStrip.Children.Add(QuestChip(quest));
            QuestStrip.Visibility = world.Quests.Count == 0 ? Visibility.Collapsed : Visibility.Visible;
            ScoutStatus.Text = $"{world.Items.Count} items across {groups.Count} floors" + (query.Requirements.Count == 0 ? "" : $"  ·  {matches.Count} requirement match{(matches.Count == 1 ? "" : "es")}");
            EmptyScout.Visibility = Visibility.Collapsed; ScoutList.Visibility = Visibility.Visible;
            renderedSeed = seed;
        }
        catch (Exception ex)
        {
            if (generation != scoutGeneration) return;
            ScoutStatus.Text = ex.Message;
            // Keep the indicator describing the manifest that is still shown.
            scoutedSeed = renderedSeed; UpdateResultNav();
        }
        finally { if (generation == scoutGeneration) ScoutButton.IsEnabled = SeedCode.IsCanonical(SeedInput.Text); }
    }
    private static string Region(int depth) => depth switch { <= 5 => "Sewers", <= 10 => "Prison", <= 15 => "Caves", <= 20 => "Dwarven City", _ => "Demon Halls" };
    /// <summary>The variant label of the quest hosted on <paramref name="depth"/>, or "" for quest-less floors.</summary>
    private static string QuestLabel(IReadOnlyList<ScoutQuest> quests, int depth) =>
        quests.FirstOrDefault(quest => quest.Depth == depth) is { } quest ? ScoutQuests.VariantLabel(quest.Variant) : "";
    /// <summary>A pill summarising one quest, e.g. "Great crab · Sad Ghost · F4".</summary>
    private static Border QuestChip(ScoutQuest quest)
    {
        var text = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        text.Children.Add(new TextBlock { Text = ScoutQuests.VariantLabel(quest.Variant), FontSize = 11, FontWeight = FontWeights.SemiBold });
        text.Children.Add(new TextBlock
        {
            Text = $"· {ScoutQuests.GiverLabel(quest.Giver)} · F{quest.Depth}",
            FontSize = 11,
            Foreground = ThemeBrush("TextFillColorSecondaryBrush", Microsoft.UI.Colors.Gray),
        });
        return new Border
        {
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(8, 2, 8, 2),
            Background = ThemeBrush("LayerFillColorDefaultBrush", Microsoft.UI.Colors.Transparent),
            Child = text,
        };
    }
    private void CopySeed_Click(object sender, RoutedEventArgs e) { if (SeedCode.IsCanonical(SeedInput.Text)) Copy(SeedInput.Text); }
    private static void Copy(string text) { var data = new DataPackage(); data.SetText(text); Clipboard.SetContent(data); }
}

public sealed class ScoutGroup : List<ScoutRow>
{
    public string Floor { get; init; } = "";
    public string Region { get; init; } = "";
    /// <summary>The floor's quest variant label, or "" when it hosts no quest.</summary>
    public string Quest { get; init; } = "";
}

public sealed class ScoutRow
{
    public string ItemName { get; init; } = "";
    public string Upgrade { get; init; } = "";
    public Visibility UpgradeVisibility { get; init; } = Visibility.Collapsed;
    public Visibility CurseVisibility { get; init; } = Visibility.Collapsed;
    public Visibility SecretVisibility { get; init; } = Visibility.Collapsed;
    public string Effect { get; init; } = "";
    public Visibility EffectVisibility { get; init; } = Visibility.Collapsed;
    public Brush EffectBrush { get; init; } = new SolidColorBrush(Color.FromArgb(255, 42, 160, 176));
    public string Source { get; init; } = "";
    public string Accessibility { get; init; } = "";
    public Visibility AccessibilityVisibility { get; init; } = Visibility.Collapsed;
    public Visibility MatchVisibility { get; init; } = Visibility.Collapsed;
    /// <summary>Row-major index into the upstream item atlas.</summary>
    public int SpriteIndex { get; init; } = -1;
    /// <summary>Enchantment/curse glow colour; only meaningful when <see cref="GlowPeriod"/> is positive.</summary>
    public Color GlowColor { get; init; }
    /// <summary>Seconds to peak glow, or zero when the item neither is enchanted nor cursed.</summary>
    public double GlowPeriod { get; init; }
    public Windows.UI.Text.FontWeight Weight { get; init; } = FontWeights.Normal;

    public static ScoutRow From(ScoutItem x, bool match)
    {
        var access = x.AccessibilityTag switch { 1 => $"One reward of choice group {x.AccessibilityGroup} (option {x.AccessibilityValue + 1})", 2 => $"Only in some outcomes of scenario group {x.AccessibilityGroup}", _ => "" };
        var isCurse = x.Effect is not null && ItemCatalog.IsCurse(x.Item.Kind, x.Effect);
        var glow = ItemGlow.ForItem(x);
        return new()
        {
            ItemName = x.Item.Name,
            Upgrade = $"+{x.Upgrade}", UpgradeVisibility = x.Upgrade > 0 ? Visibility.Visible : Visibility.Collapsed,
            CurseVisibility = x.Cursed ? Visibility.Visible : Visibility.Collapsed,
            SecretVisibility = x.Secret ? Visibility.Visible : Visibility.Collapsed,
            Effect = x.Effect ?? "", EffectVisibility = x.Effect is null ? Visibility.Collapsed : Visibility.Visible,
            EffectBrush = isCurse ? (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"] : new SolidColorBrush(Color.FromArgb(255, 42, 160, 176)),
            Source = Labels.Source(x.Source),
            Accessibility = access, AccessibilityVisibility = access.Length == 0 ? Visibility.Collapsed : Visibility.Visible,
            MatchVisibility = match ? Visibility.Visible : Visibility.Collapsed,
            Weight = match ? FontWeights.SemiBold : FontWeights.Normal,
            SpriteIndex = x.Item.SpriteIndex,
            GlowColor = glow?.Color ?? default, GlowPeriod = glow?.Period ?? 0,
        };
    }
}
