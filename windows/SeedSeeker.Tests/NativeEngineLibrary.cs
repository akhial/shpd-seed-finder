using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace SeedSeeker.Tests;

/// <summary>
/// Points the linked <c>NativeEngine.cs</c> P/Invokes at the cargo-built engine
/// library for whatever host is running the tests. The app ships the DLL beside
/// its binary (see the WinUI csproj), but these tests run unpackaged and on
/// macOS/Linux too, where the file is <c>libshpd_seedfinder_ffi.dylib</c> /
/// <c>.so</c> in the workspace target directory. The csproj builds it before the
/// tests run; this only has to find it.
/// </summary>
internal static class NativeEngineLibrary
{
    private const string Library = "shpd_seedfinder_ffi";

    [ModuleInitializer]
    internal static void Register() =>
        NativeLibrary.SetDllImportResolver(typeof(NativeEngineLibrary).Assembly, Resolve);

    private static nint Resolve(string name, Assembly assembly, DllImportSearchPath? path)
    {
        if (name != Library) return 0;
        foreach (var candidate in Candidates())
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle)) return handle;
        throw new DllNotFoundException(
            $"Could not find the engine library. Build it with `cargo build -p shpd-seedfinder-ffi`. Looked in:{Environment.NewLine}"
            + string.Join(Environment.NewLine, Candidates()));
    }

    private static IEnumerable<string> Candidates()
    {
        var file = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? $"{Library}.dll"
            : RuntimeInformation.IsOSPlatform(OSPlatform.OSX) ? $"lib{Library}.dylib" : $"lib{Library}.so";
        foreach (var directory in TargetDirectories())
            foreach (var profile in new[] { "debug", "release" })
                yield return Path.Combine(directory, profile, file);
    }

    /// <summary>Where cargo puts its output: the override first, else the workspace default.</summary>
    private static IEnumerable<string> TargetDirectories()
    {
        if (Environment.GetEnvironmentVariable("CARGO_TARGET_DIR") is { Length: > 0 } overridden)
            yield return Path.GetFullPath(overridden);
        if (WorkspaceRoot() is { } root) yield return Path.Combine(root, "target");
    }

    /// <summary>The repository root, found by walking up from the test binary.</summary>
    private static string? WorkspaceRoot()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
            if (File.Exists(Path.Combine(directory.FullName, "Cargo.toml"))
                && Directory.Exists(Path.Combine(directory.FullName, "crates"))) return directory.FullName;
        return null;
    }
}
