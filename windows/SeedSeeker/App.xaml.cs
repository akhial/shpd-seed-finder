using Microsoft.UI.Xaml;

namespace SeedSeeker;

public partial class App : Application
{
    private Window? window;
    public App()
    {
        UnhandledException += (_, e) =>
        {
            try { File.WriteAllText(Path.Combine(Path.GetTempPath(), "SeedSeeker-crash.txt"), e.Exception.ToString()); } catch { }
        };
        InitializeComponent();
    }
    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new MainWindow();
        window.Activate();
    }
}
