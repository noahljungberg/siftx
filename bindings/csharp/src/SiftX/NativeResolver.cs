using System.Reflection;
using System.Runtime.InteropServices;

namespace SiftX;

/// <summary>
/// Registers a custom NativeLibrary resolver that searches additional paths
/// for the siftx native library (development builds, NuGet runtimes).
/// </summary>
internal static class NativeResolver
{
    private static int _registered;

    internal static void EnsureRegistered()
    {
        if (Interlocked.Exchange(ref _registered, 1) == 0)
        {
            NativeLibrary.SetDllImportResolver(typeof(NativeResolver).Assembly, Resolve);
        }
    }

    private static nint Resolve(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != "siftx")
            return 0;

        nint handle;

        // 1. Try SIFTX_NATIVE_LIB_PATH environment variable (explicit override)
        var envPath = Environment.GetEnvironmentVariable("SIFTX_NATIVE_LIB_PATH");
        if (envPath is not null && NativeLibrary.TryLoad(envPath, out handle))
            return handle;

        // 2. Try well-known paths relative to the managed assembly.
        //    On Windows the managed assembly is also named SiftX.dll, which
        //    collides case-insensitively with the native siftx.dll. Placing
        //    the native library under runtimes/<rid>/native/ (the standard
        //    NuGet layout) avoids the collision, so we probe there first.
        var asmDir = Path.GetDirectoryName(assembly.Location) ?? ".";

        string[] candidates;
        if (OperatingSystem.IsWindows())
            candidates = ["siftx.dll"];
        else if (OperatingSystem.IsMacOS())
            candidates = ["libsiftx.dylib", "libsiftx.so"];
        else
            candidates = ["libsiftx.so"];

        foreach (var name in candidates)
        {
            // runtimes/<rid>/native/ (NuGet layout - checked first to avoid
            // the Windows name collision with the managed SiftX.dll).
            //
            // Probe every plausible RID, not just the running one. On .NET 8/9
            // RuntimeInformation.RuntimeIdentifier reports a distro-specific RID
            // ("ubuntu.24.04-x64") while the SDK stages native assets under the
            // portable RID ("linux-x64"), so trusting the running RID alone
            // finds nothing on exactly the frameworks we also target.
            foreach (var rid in ProbeRids())
            {
                var ridPath = Path.Combine(asmDir, "runtimes", rid, "native", name);
                if (NativeLibrary.TryLoad(ridPath, out handle))
                    return handle;
            }

            // Directly next to the assembly (Linux/macOS dev builds)
            var path = Path.Combine(asmDir, name);
            if (NativeLibrary.TryLoad(path, out handle))
                return handle;

            // Any staged RID directory - covers cross-RID publish layouts
            // and RIDs newer than this assembly knows about.
            var runtimesDir = Path.Combine(asmDir, "runtimes");
            if (Directory.Exists(runtimesDir))
            {
                foreach (var dir in Directory.EnumerateDirectories(runtimesDir))
                {
                    var anyPath = Path.Combine(dir, "native", name);
                    if (NativeLibrary.TryLoad(anyPath, out handle))
                        return handle;
                }
            }
        }

        // 3. Fall back to default OS search (PATH, LD_LIBRARY_PATH, etc.)
        if (NativeLibrary.TryLoad(libraryName, assembly, searchPath, out handle))
            return handle;

        return 0;
    }

    /// <summary>
    /// RIDs to probe, most specific first: the running RID, then the portable
    /// "{os}-{arch}" RID the SDK actually stages native assets under.
    /// </summary>
    private static IEnumerable<string> ProbeRids()
    {
        var running = RuntimeInformation.RuntimeIdentifier;
        yield return running;

        string os;
        if (OperatingSystem.IsWindows()) os = "win";
        else if (OperatingSystem.IsMacOS()) os = "osx";
        else if (OperatingSystem.IsLinux()) os = "linux";
        else yield break;

        var arch = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => "x64",
            Architecture.X86 => "x86",
            Architecture.Arm64 => "arm64",
            Architecture.Arm => "arm",
            _ => null,
        };
        if (arch is null)
            yield break;

        var portable = $"{os}-{arch}";
        if (portable != running)
            yield return portable;
    }
}

// Module initializer to register resolver before any P/Invoke calls
file static class ModuleInit
{
    [System.Runtime.CompilerServices.ModuleInitializer]
    internal static void Init() => NativeResolver.EnsureRegistered();
}
