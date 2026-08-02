using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;

namespace SeedSeeker;

public partial class App : Application
{
    private MainWindow? window;
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
        // Activations redirected from later instances arrive on a worker
        // thread; hop to the UI thread before touching the window.
        AppInstance.GetCurrent().Activated += (_, activation) =>
            window?.DispatcherQueue.TryEnqueue(() => OpenActivation(activation));
        window.Activate();
        OpenActivation(AppInstance.GetCurrent().GetActivatedEventArgs());
    }

    /// <summary>Routes a seedseeker:// activation's link to the main window.</summary>
    private void OpenActivation(AppActivationArguments activation)
    {
        if (activation.Kind == ExtendedActivationKind.Protocol
            && activation.Data is Windows.ApplicationModel.Activation.ProtocolActivatedEventArgs protocol)
            window?.OpenSharedLink(protocol.Uri.OriginalString);
    }
}
