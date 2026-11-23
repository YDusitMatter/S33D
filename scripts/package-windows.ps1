#Requires -Version 5.1
[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?$')]
    [string]$Version = '0.1.0-alpha.1',
    [string]$ReleaseDirectory,
    [Parameter(Mandatory = $true)]
    [string]$SourceBundlePath,
    [string]$OutputDirectory,
    [string]$IsccPath,
    [string]$DumpbinPath,
    [string]$RuntimeDirectory,
    [switch]$StageOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Get-AbsolutePath([string]$Path) {
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Get-RequiredFile([string]$Path) {
    $resolved = Get-AbsolutePath $Path
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw "Required file is missing: $resolved"
    }
    return $resolved
}

function Find-VisualStudio {
    $programs = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
    if (-not $programs) { return $null }
    $vswhere = Join-Path $programs 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) { return $null }
    $result = @(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
    if ($LASTEXITCODE -ne 0 -or $result.Count -eq 0) { return $null }
    return ([string]$result[0]).Trim()
}

function Find-Tool([string]$ExplicitPath, [string]$Name, [string[]]$Candidates) {
    if ($ExplicitPath) { return Get-RequiredFile $ExplicitPath }
    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command) { return $command.Source }
    foreach ($candidate in $Candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return $candidate }
    }
    throw "$Name was not found. Supply its explicit path; this script does not download or install tools."
}

function Assert-X64([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $reader = New-Object IO.BinaryReader($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE image: $Path" }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
            throw "Expected a Windows x64 PE image: $Path"
        }
    } finally { $reader.Dispose(); $stream.Dispose() }
}

function Get-Imports([string]$Path) {
    $lines = @(& $script:resolvedDumpbin /nologo /dependents $Path 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Cannot inspect dependencies of $Path`n$($lines -join [Environment]::NewLine)" }
    return @($lines | ForEach-Object {
        if ([string]$_ -match '^\s+([A-Za-z0-9_.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
    } | Sort-Object -Unique)
}

if (-not $ReleaseDirectory) { $ReleaseDirectory = Join-Path $repoRoot 'target\release' }
$releaseRoot = Get-AbsolutePath $ReleaseDirectory
$appExe = Get-RequiredFile (Join-Path $releaseRoot 's33d-app.exe')
$agentExe = Get-RequiredFile (Join-Path $releaseRoot 's33d-agent.exe')
$sourceZip = Get-RequiredFile $SourceBundlePath
if ([IO.Path]::GetExtension($sourceZip) -ne '.zip') { throw 'SourceBundlePath must name the corresponding source ZIP.' }

# All runtime content is explicitly listed. Never copy the workspace recursively.
$payloadFiles = @(
    'LICENSE',
    'THIRD_PARTY_LICENSES.md',
    'README.md',
    'assets/icons/s33d-seed.ico',
    'assets/icons/s33d-sprouted.ico',
    'assets/fonts/OFL-NotoSansSC.txt',
    'assets/licenses/occt/LICENSE_LGPL_21.txt',
    'assets/licenses/occt/OCCT_LGPL_EXCEPTION.txt',
    'Docs/UserManual.md',
    'Docs/UserManual.en.md',
    'Docs/UserManual.zh-CN.md',
    'Docs/AgentControl.md', 'Docs/PhotoToModel.md',
    'Docs/ExactGeometryBuild.md',
    'Docs/FastenerGenerators.md',
    'Docs/ThreadPrinting.md',
    'packaging/RELEASE-NOTES.md',
    'agent-config/codex.toml.example',
    'agent-config/claude-code.mcp.json.example',
    'skills/s33d-modeling/SKILL.md',
    'skills/s33d-modeling/agents/openai.yaml',
    'skills/s33d-modeling/references/modeling-patterns.md',
    'skills/s33d-modeling/references/modeling-strategy.md',
    'skills/s33d-modeling/references/photo-reconstruction.md',
    'skills/s33d-modeling/references/visual-validation.md',
    'skills/s33d-modeling/references/interface-contracts.md'
)
$pluginIds = @('calculator', 'gear-generator', 'surface-texture', 'tire-generator', 'wheel-rim-generator', 'screw-generator', 'nut-generator')
foreach ($pluginId in $pluginIds) {
    $payloadFiles += "Plug-ins/$pluginId/plugin.json", "Plug-ins/$pluginId/README.md"
    $manifestPath = Get-RequiredFile "Plug-ins/$pluginId/plugin.json"
    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.id -ne $pluginId -or $manifest.entry -ne "builtin:$pluginId") {
        throw "Unsupported built-in plug-in manifest: $manifestPath"
    }
}
foreach ($relative in $payloadFiles) { $null = Get-RequiredFile $relative }
$sourceReadme = Get-RequiredFile 'packaging/SOURCE-README.md'
$installReadme = Get-RequiredFile 'packaging/INSTALL-README.txt'
$installerScript = Get-RequiredFile 'packaging/windows/S33D.iss'

Assert-X64 $appExe
Assert-X64 $agentExe
$productVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($appExe).ProductVersion
if ($productVersion -ne $Version) {
    throw "s33d-app.exe ProductVersion is '$productVersion'; expected '$Version'. Build both release binaries with the final icons first."
}
$agentVersion = @(& $agentExe --version 2>&1)
if ($LASTEXITCODE -ne 0 -or ($agentVersion -join '').Trim() -ne "s33d-agent $Version") {
    throw "s33d-agent.exe does not report the expected version '$Version'. Rebuild both binaries together."
}

$vsRoot = Find-VisualStudio
$dumpbinCandidates = @()
if ($vsRoot) {
    $toolsets = @(Get-ChildItem -LiteralPath (Join-Path $vsRoot 'VC\Tools\MSVC') -Directory | Sort-Object Name -Descending)
    foreach ($toolset in $toolsets) { $dumpbinCandidates += Join-Path $toolset.FullName 'bin\Hostx64\x64\dumpbin.exe' }
}
$script:resolvedDumpbin = Find-Tool $DumpbinPath 'dumpbin.exe' $dumpbinCandidates
if (-not $RuntimeDirectory -and $vsRoot) {
    $redistRoot = Join-Path $vsRoot 'VC\Redist\MSVC'
    if (Test-Path -LiteralPath $redistRoot -PathType Container) {
        foreach ($redistVersion in (Get-ChildItem -LiteralPath $redistRoot -Directory | Sort-Object Name -Descending)) {
            $x64Directory = Join-Path $redistVersion.FullName 'x64'
            if (-not (Test-Path -LiteralPath $x64Directory -PathType Container)) { continue }
            $crt = Get-ChildItem -LiteralPath $x64Directory -Directory | Where-Object { $_.Name -match '^Microsoft\.VC\d+\.CRT$' } | Select-Object -First 1
            if ($crt) { $RuntimeDirectory = $crt.FullName; break }
        }
    }
}
$runtimeFiles = @{}
if ($RuntimeDirectory) {
    $runtimeRoot = Get-AbsolutePath $RuntimeDirectory
    # Use a redistributable input directory; never harvest DLLs from Windows.
    $windowsRoot = [IO.Path]::GetFullPath([Environment]::GetFolderPath('Windows')).TrimEnd('\') + '\'
    if ($runtimeRoot.StartsWith($windowsRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'RuntimeDirectory must be a redistributable runtime directory, not a Windows system directory.'
    }
    foreach ($dll in (Get-ChildItem -LiteralPath $runtimeRoot -File -Filter '*.dll')) {
        $runtimeFiles[$dll.Name.ToLowerInvariant()] = $dll.FullName
    }
}

$systemDlls = @(
    'advapi32.dll', 'avrt.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'cfgmgr32.dll',
    'comctl32.dll', 'comdlg32.dll', 'crypt32.dll', 'cryptbase.dll', 'd2d1.dll',
    'd3d11.dll', 'd3d12.dll', 'd3dcompiler_47.dll', 'dbghelp.dll', 'dcomp.dll',
    'dnsapi.dll', 'dwmapi.dll', 'dwrite.dll', 'dxcore.dll', 'dxgi.dll', 'gdi32.dll',
    'hid.dll', 'imm32.dll', 'iphlpapi.dll', 'kernel32.dll', 'kernelbase.dll',
    'msimg32.dll', 'msvcrt.dll', 'ncrypt.dll', 'netapi32.dll', 'normaliz.dll',
    'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'opengl32.dll', 'powrprof.dll',
    'propsys.dll', 'psapi.dll', 'rpcrt4.dll', 'secur32.dll', 'setupapi.dll',
    'shcore.dll', 'shell32.dll', 'shlwapi.dll', 'ucrtbase.dll', 'uiautomationcore.dll',
    'user32.dll', 'userenv.dll', 'usp10.dll', 'uxtheme.dll', 'version.dll',
    'windowscodecs.dll', 'winhttp.dll', 'wininet.dll', 'winmm.dll', 'winspool.drv',
    'wintrust.dll', 'ws2_32.dll', 'wtsapi32.dll'
)
$pending = New-Object 'Collections.Generic.Queue[string]'
$pending.Enqueue($appExe)
$pending.Enqueue($agentExe)
$dependencies = [ordered]@{}
$bundledDlls = @{}
while ($pending.Count -gt 0) {
    $binary = $pending.Dequeue()
    $imports = @(Get-Imports $binary)
    $dependencies[[IO.Path]::GetFileName($binary)] = $imports
    foreach ($dependency in $imports) {
        if ($dependency -match '^(api-ms-|ext-ms-)' -or $systemDlls -contains $dependency) { continue }
        if ($bundledDlls.ContainsKey($dependency)) { continue }
        if (-not $runtimeFiles.ContainsKey($dependency)) {
            throw "Missing redistributable dependency '$dependency' required by $binary. Supply -RuntimeDirectory with reviewed x64 DLLs."
        }
        $dllPath = $runtimeFiles[$dependency]
        Assert-X64 $dllPath
        $bundledDlls[$dependency] = $dllPath
        $pending.Enqueue($dllPath)
    }
}

$resolvedIscc = $null
if (-not $StageOnly) {
    $isccCandidates = @((Join-Path $repoRoot 'target\packaging-tools\InnoSetup\ISCC.exe'))
    foreach ($programs in @([Environment]::GetEnvironmentVariable('ProgramFiles(x86)'), [Environment]::GetEnvironmentVariable('ProgramFiles'), [Environment]::GetEnvironmentVariable('LOCALAPPDATA'))) {
        if ($programs) {
            $isccCandidates += Join-Path $programs 'Inno Setup 6\ISCC.exe'
            $isccCandidates += Join-Path $programs 'Programs\Inno Setup 6\ISCC.exe'
        }
    }
    $resolvedIscc = Find-Tool $IsccPath 'ISCC.exe' $isccCandidates
}

if (-not $OutputDirectory) {
    $runName = '{0}-{1}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $OutputDirectory = Join-Path $repoRoot "target\packages\$runName"
}
$runRoot = Get-AbsolutePath $OutputDirectory
if (Test-Path -LiteralPath $runRoot) { throw "OutputDirectory already exists; use a new directory to preserve earlier packages: $runRoot" }
$stageRoot = Join-Path $runRoot 'stage'
$installerOutput = Join-Path $runRoot 'installer'
$null = New-Item -ItemType Directory -Path $stageRoot, $installerOutput
foreach ($relative in $payloadFiles) {
    $destination = Join-Path $stageRoot $relative
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force
    Copy-Item -LiteralPath (Join-Path $repoRoot $relative) -Destination $destination
}
Copy-Item -LiteralPath $appExe, $agentExe -Destination $stageRoot
foreach ($dependency in ($bundledDlls.Keys | Sort-Object)) {
    Copy-Item -LiteralPath $bundledDlls[$dependency] -Destination (Join-Path $stageRoot $dependency)
}
$sourceDirectory = Join-Path $stageRoot 'source'
$null = New-Item -ItemType Directory -Path $sourceDirectory
Copy-Item -LiteralPath $sourceZip -Destination (Join-Path $sourceDirectory "S33D-$Version-source.zip")
Copy-Item -LiteralPath $sourceReadme -Destination (Join-Path $sourceDirectory 'README.md')
Copy-Item -LiteralPath $installReadme -Destination (Join-Path $stageRoot 'INSTALL-README.txt')

$inventory = @(Get-ChildItem -LiteralPath $stageRoot -File -Recurse | Sort-Object FullName | ForEach-Object {
    [ordered]@{
        path = $_.FullName.Substring($stageRoot.Length + 1).Replace('\', '/')
        bytes = $_.Length
        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
})
$packageManifest = [ordered]@{
    product = 'S33D Alpha Preview'
    version = $Version
    publisher = 'YDZM'
    architecture = 'x86_64-pc-windows-msvc'
    created_utc = [DateTime]::UtcNow.ToString('o')
    dependencies = $dependencies
    files = $inventory
}
$packageManifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $stageRoot 'package-manifest.json') -Encoding UTF8

$setupExe = $null
if (-not $StageOnly) {
    $numericVersion = (($Version -split '-')[0]) + '.0'
    & $resolvedIscc "/DAppVersion=$Version" "/DFileVersion=$numericVersion" "/DStageDir=$stageRoot" "/DPackageOutputDir=$installerOutput" $installerScript
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup compilation failed; staged files are preserved at $stageRoot" }
    $setupExe = Get-RequiredFile (Join-Path $installerOutput "S33D-$Version-windows-x64-setup.exe")
    $setupHash = Get-FileHash -LiteralPath $setupExe -Algorithm SHA256
    ('{0}  {1}' -f $setupHash.Hash.ToLowerInvariant(), [IO.Path]::GetFileName($setupExe)) |
        Set-Content -LiteralPath ($setupExe + '.sha256') -Encoding UTF8
}
[pscustomobject]@{
    Version = $Version
    StageDirectory = $stageRoot
    Installer = $setupExe
    BundledRuntimeDlls = @($bundledDlls.Keys | Sort-Object)
    PayloadFileCount = $inventory.Count + 1
}
