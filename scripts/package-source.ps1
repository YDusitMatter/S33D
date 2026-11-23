#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$OutputDirectory,
    [string]$OcctCratePath,
    [switch]$ValidateOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$metadataText = & cargo metadata --locked --offline --format-version 1 --manifest-path (Join-Path $repo 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Cannot read locked Cargo metadata.' }
$metadata = ($metadataText -join "`n") | ConvertFrom-Json
$app = $metadata.packages | Where-Object name -eq 's33d-app' | Select-Object -First 1
$occt = $metadata.packages | Where-Object name -eq 'occt-sys' | Select-Object -First 1
if (-not $app -or -not $occt) { throw 'Missing application or OCCT source package.' }
$version = $app.version
$occtVersion = $occt.version
$lockText = Get-Content -LiteralPath (Join-Path $repo 'Cargo.lock') -Raw -Encoding UTF8
$lockBlock = $lockText -split '\[\[package\]\]' | Where-Object { $_ -match '(?m)^name = "occt-sys"\r?$' }
if (@($lockBlock).Count -ne 1 -or $lockBlock -notmatch '(?m)^checksum = "([a-f0-9]{64})"\r?$') {
    throw 'Cannot establish the locked occt-sys archive checksum.'
}
$expectedChecksum = $Matches[1]
if (-not $OcctCratePath) {
    $crateDirectory = Split-Path -Parent $occt.manifest_path
    $registryDirectory = Split-Path -Parent $crateDirectory
    $registryName = Split-Path -Leaf $registryDirectory
    $registryRoot = Split-Path -Parent (Split-Path -Parent $registryDirectory)
    $OcctCratePath = Join-Path $registryRoot "cache\$registryName\occt-sys-$occtVersion.crate"
}
if (-not (Test-Path -LiteralPath $OcctCratePath -PathType Leaf)) {
    throw 'The matching published occt-sys archive is missing. Fetch locked dependencies, or pass -OcctCratePath.'
}
if ((Get-FileHash -LiteralPath $OcctCratePath -Algorithm SHA256).Hash -ne $expectedChecksum) {
    throw 'occt-sys archive does not match Cargo.lock; refusing to distribute mismatched sources.'
}

# Public source whitelist: never copy the repository root recursively.
$directories = @('crates', 'assets', 'Plug-ins', 'agent-config', 'skills/s33d-modeling', 'scripts')
$relativeFiles = @(
    'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md', 'THIRD_PARTY_LICENSES.md',
    '.gitignore', '.gitattributes', 'Docs/Overview.md',
    'Docs/UserManual.md', 'Docs/UserManual.en.md', 'Docs/UserManual.zh-CN.md',
    'Docs/AgentControl.md', 'Docs/PhotoToModel.md', 'Docs/ExactGeometryBuild.md', 'Docs/FastenerGenerators.md',
    'Docs/ThreadPrinting.md', 'Docs/SolidWorksCompatibility.md', 'Docs/LinuxPackaging.md',
    'packaging/README.md', 'packaging/SOURCE-README.md', 'packaging/RELEASE-NOTES.md',
    'packaging/INSTALL-README.txt', 'packaging/windows/S33D.iss'
)
function Get-PublicFiles([string]$Directory) {
    foreach ($item in Get-ChildItem -LiteralPath $Directory -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Unexpected source reparse point: $($item.Name)" }
        if ($item.Name -match '^(\.git|\.reasonix|\.agents|\.codex|target|backups|debug|Logs|work|tmp|output|ui_state\.json|reasonix\.toml|credentials(?:\.json)?|\.mcp\.json)$') { continue }
        if ($item.Name -match '^\.env(?:\..*)?$' -and $item.Name -ne '.env.example') { continue }
        if ($item.PSIsContainer) { Get-PublicFiles $item.FullName }
        elseif ($item.Extension -notin @('.log','.trace','.tmp','.temp','.dmp','.bak','.pdb','.key','.pem','.pfx','.p12','.s33d','.step','.stp','.iges','.igs','.stl')) { $item }
    }
}
$sourceFiles = @($relativeFiles | ForEach-Object { Get-Item -LiteralPath (Join-Path $repo $_) })
foreach ($directory in $directories) { $sourceFiles += @(Get-PublicFiles (Join-Path $repo $directory)) }
$sourceFiles = @($sourceFiles | Sort-Object FullName -Unique)
$iconNames = @('s33d-artwork.png','s33d-seed.png','s33d-seed.ico','s33d-sprouted.png','s33d-sprouted.ico','brand-manifest.json')
$missingIcons = @($iconNames | Where-Object { -not (Test-Path -LiteralPath (Join-Path $repo "assets/icons/$_") -PathType Leaf) })
if ($ValidateOnly) {
    [pscustomobject]@{Version=$version;PublicFiles=$sourceFiles.Count;OcctVersion=$occtVersion;OcctChecksumVerified=$true;MissingIcons=$missingIcons}
    return
}
if ($missingIcons.Count -gt 0) { throw "Final icon assets are still missing: $($missingIcons -join ', '). Source release is not ready." }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo ('target/source-bundle/' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'Output directory already exists; choose a new one.' }
$stage = Join-Path $output 'source'
$null = New-Item -ItemType Directory -Path (Join-Path $stage 'S33D'), (Join-Path $stage 'third-party')
foreach ($file in $sourceFiles) {
    $relative = $file.FullName.Substring($repo.Length + 1)
    $destination = Join-Path $stage "S33D/$relative"
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force
    Copy-Item -LiteralPath $file.FullName -Destination $destination
}
# Keep the complete checksum-verified published crate, which contains the OCCT
# source actually supplied by the locked build dependency. Not an upstream Git clone.
Copy-Item -LiteralPath $OcctCratePath -Destination (Join-Path $stage "third-party/occt-sys-$occtVersion.crate")
Copy-Item -LiteralPath (Join-Path $repo 'packaging/SOURCE-README.md') -Destination (Join-Path $stage 'README.md')
$inventory = @(Get-ChildItem -LiteralPath $stage -Recurse -File | ForEach-Object {
    [ordered]@{path=$_.FullName.Substring($stage.Length+1).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}
})
[ordered]@{version=$version;occt_crate_version=$occtVersion;occt_crate_sha256=$expectedChecksum;files=$inventory} |
    ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $stage 'source-manifest.json') -Encoding UTF8
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = Join-Path $output "S33D-$version-source.zip"
[IO.Compression.ZipFile]::CreateFromDirectory($stage,$zip,[IO.Compression.CompressionLevel]::Optimal,$false)
$check = [IO.Compression.ZipFile]::OpenRead($zip)
try {
    foreach ($entry in $check.Entries) {
        if ($entry.FullName.EndsWith('/')) { continue }
        $stream=$entry.Open(); $digest=[Security.Cryptography.SHA256]::Create()
        try { $actual=[BitConverter]::ToString($digest.ComputeHash($stream)).Replace('-','') }
        finally { $digest.Dispose(); $stream.Dispose() }
        if ($actual -ne (Get-FileHash -LiteralPath (Join-Path $stage $entry.FullName)).Hash) { throw 'Source archive verification failed.' }
    }
} finally { $check.Dispose() }
[pscustomobject]@{SourceBundle=$zip;SHA256=(Get-FileHash -LiteralPath $zip).Hash;Version=$version}
