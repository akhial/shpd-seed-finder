param(
    [ValidateSet('Debug','Release')][string]$Configuration = 'Release',
    # Defaults to the host architecture; pass ARM64 or x64 to cross-build.
    [ValidateSet('ARM64','x64')][string]$Platform = $(
        if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'ARM64' } else { 'x64' }
    )
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:DOTNET_CLI_HOME = Join-Path $root '.dotnet-home'
$env:NUGET_PACKAGES = Join-Path $root '.nuget-packages'
dotnet build "$root\windows\SeedSeeker\SeedSeeker.csproj" -c $Configuration -p:Platform=$Platform
