using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;

namespace SeedSeeker;

/// <summary>
/// Hand-written entry point (the csproj defines DISABLE_XAML_GENERATED_MAIN)
/// so a seedseeker:// activation reaches the one running instance instead of
/// opening another window.
/// </summary>
public static class Program
{
    [STAThread]
    private static void Main()
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        var activation = AppInstance.GetCurrent().GetActivatedEventArgs();
        var primary = AppInstance.FindOrRegisterForKey("main");
        if (!primary.IsCurrent)
        {
            // Hand the activation to the running instance, then exit. This
            // process never shows UI, so blocking its thread is fine.
            primary.RedirectActivationToAsync(activation).AsTask().Wait();
            return;
        }
        // Register the seedseeker:// scheme for the current user. Unpackaged
        // apps must self-register; the writes are idempotent HKCU entries, so
        // repeating them every launch is safe and keeps the registration
        // pointing at wherever the executable currently lives. An empty
        // executable path means the current process's own.
        try
        {
            ActivationRegistrationManager.RegisterForProtocolActivation(
                "seedseeker",
                Path.Combine(AppContext.BaseDirectory, "Assets", "SeedSeeker.ico"),
                "Seed Seeker",
                "");
        }
        catch
        {
            // A locked-down registry only costs link activation, not the app.
        }
        Application.Start(_ =>
        {
            SynchronizationContext.SetSynchronizationContext(
                new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            new App();
        });
    }
}
