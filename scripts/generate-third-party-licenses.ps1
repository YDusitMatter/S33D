param(
    [string]$OutputPath = "THIRD_PARTY_LICENSES.md"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$outputFile = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path $repoRoot $OutputPath
}

Push-Location $repoRoot
try {
    $metadata = cargo metadata --format-version 1 --locked --offline | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed with exit code $LASTEXITCODE"
    }

    $workspaceIds = @($metadata.workspace_members)
    $packages = @(
        $metadata.packages |
            Where-Object { $workspaceIds -notcontains $_.id } |
            Sort-Object name, version
    )
    $missingLicenses = @($packages | Where-Object { [string]::IsNullOrWhiteSpace($_.license) })

    $fontPackage = $packages |
        Where-Object { $_.name -eq "epaint_default_fonts" } |
        Sort-Object version -Descending |
        Select-Object -First 1
    if (-not $fontPackage) {
        throw "epaint_default_fonts was not found in the locked dependency graph"
    }
    $fontRoot = Split-Path -Parent $fontPackage.manifest_path

    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add("# Third-Party Licenses")
    $lines.Add("")
    $lines.Add("This inventory covers the third-party Rust packages locked by ``Cargo.lock`` and the fonts embedded in S33D through ``epaint_default_fonts``. It was generated on $(Get-Date -Format 'yyyy-MM-dd') by ``scripts/generate-third-party-licenses.ps1``.")
    $lines.Add("")
    $lines.Add("The license expressions below are copied from Cargo package metadata. They are an inventory, not a replacement for the complete upstream license and notice files. Distributions should ship this file together with S33D's root ``LICENSE``.")
    $lines.Add("")
    $lines.Add("## Embedded fonts")
    $lines.Add("")
    $lines.Add("S33D enables egui's ``default_fonts`` feature. The following font files from ``epaint_default_fonts`` $($fontPackage.version) are compiled into the executable:")
    $lines.Add("")
    $lines.Add("| Embedded font | Upstream license | Copyright / source |")
    $lines.Add("| --- | --- | --- |")
    $lines.Add("| Hack Regular | MIT plus Bitstream Vera terms; DejaVu portions are public domain | Copyright 2018 Source Foundry Authors; Bitstream Vera copyright 2003 Bitstream, Inc. |")
    $lines.Add("| Noto Emoji Regular | SIL Open Font License 1.1 (``OFL-1.1``) | Noto font family / Google |")
    $lines.Add("| Ubuntu Light | Ubuntu Font Licence 1.0 (``Ubuntu-font-1.0``) | Ubuntu typeface |")
    $lines.Add("| emoji-icon-font | MIT | Copyright 2014 John Slegers |")
    $lines.Add("")
    $lines.Add("The full notices supplied with those fonts are reproduced in the Font license texts section below.")
    $lines.Add("")
    $lines.Add("### Sketch outline fonts")
    $lines.Add("")
    $lines.Add("Sketch Text uses the above Ubuntu Light and Hack fonts, plus unmodified Noto Sans SC (SIL Open Font License 1.1) embedded from ``assets/fonts/NotoSansSC.ttf``. The full Noto Sans SC copyright and license are in ``assets/fonts/OFL-NotoSansSC.txt``; include this notice in distributions. Drawing PDF export also embeds the same unmodified Noto Sans SC font. No Windows system fonts are used to generate exported text geometry or PDF text.")
    $lines.Add("")
    $lines.Add("Noto Sans SC source: https://github.com/google/fonts/tree/a85815a42757630ce188fdad368c2dfc444d4773/ofl/notosanssc. SHA-256: ``a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da``.")
    $lines.Add("")
    $lines.Add("### Runtime system-font fallback (not redistributed)")
    $lines.Add("")
    $lines.Add("On Windows, S33D may read SimHei, Microsoft YaHei, SimSun, Segoe UI, or Segoe UI Symbol from the user's local Windows Fonts directory to provide CJK and calculator glyph fallback. These system font files are not stored in this repository and are not embedded or redistributed by S33D.")
    $lines.Add("")
    $lines.Add("## Locked Rust dependency inventory")
    $lines.Add("")
    $lines.Add("### Native geometry dependency")
    $lines.Add("")
    $lines.Add("Open CASCADE Technology (OCCT) 7.8.1 is built from the source bundled by occt-sys 0.6.0. It is separate from S33D's MPL-2.0 license. OCCT is licensed under LGPL-2.1 with the accompanying OCCT exception; full texts are in assets/licenses/occt/LICENSE_LGPL_21.txt and assets/licenses/occt/OCCT_LGPL_EXCEPTION.txt. Upstream: https://dev.opencascade.org/.")
    $lines.Add("")
    $lines.Add("The current build links OCCT statically. This notice alone does not satisfy all binary distribution obligations. Before publishing binaries, review and provide the corresponding source, build/relink materials and notices required by the applicable licenses. See Docs/ExactGeometryBuild.md. The exception must not be described as a blanket exemption for static linking.")
    $lines.Add("S33D includes a modified OCCT Standard_Type.cxx in crates/kernel/native/occt_patches under its original LGPL/exception terms. The marked change retains the registry container through process teardown to protect late static STEP type handles. Include the modified source in corresponding source materials.")
    $lines.Add("")
    $lines.Add("This graph contains $($packages.Count) package/version records, including target-specific transitive dependencies for supported platforms.")
    $lines.Add("")
    $lines.Add("| Package | Version | Declared license | Upstream |")
    $lines.Add("| --- | --- | --- | --- |")
    foreach ($package in $packages) {
        $name = [string]$package.name
        $version = [string]$package.version
        $license = if ([string]::IsNullOrWhiteSpace($package.license)) { "NOT DECLARED" } else { [string]$package.license }
        $repository = if ([string]::IsNullOrWhiteSpace($package.repository)) {
            "https://crates.io/crates/$name/$version"
        } else {
            [string]$package.repository
        }
        $name = $name.Replace("|", "\|")
        $version = $version.Replace("|", "\|")
        $license = $license.Replace("|", "\|")
        $repository = $repository.Replace("|", "%7C")
        $lines.Add("| $name | $version | $license | <$repository> |")
    }
    $lines.Add("")
    if ($missingLicenses.Count -eq 0) {
        $lines.Add("All locked third-party package records declare a license expression in Cargo metadata.")
    } else {
        $names = ($missingLicenses | ForEach-Object { "$($_.name) $($_.version)" }) -join ", "
        $lines.Add("Packages without a declared Cargo license expression require manual review: $names")
    }
    $lines.Add("")
    $lines.Add("## Font license texts")
    $lines.Add("")

    $fontNotices = @(
        @{ Title = "Hack Regular"; File = "Hack-Regular.txt" },
        @{ Title = "Noto Emoji Regular — SIL Open Font License 1.1"; File = "OFL.txt" },
        @{ Title = "Ubuntu Light — Ubuntu Font Licence 1.0"; File = "UFL.txt" },
        @{ Title = "emoji-icon-font — MIT License"; File = "emoji-icon-font-mit-license.txt" }
    )
    foreach ($notice in $fontNotices) {
        $noticePath = Join-Path $fontRoot (Join-Path "fonts" $notice.File)
        if (-not (Test-Path -LiteralPath $noticePath)) {
            throw "Required embedded-font notice is missing: $noticePath"
        }
        $noticeText = (Get-Content -Raw -LiteralPath $noticePath).TrimEnd()
        $lines.Add("### $($notice.Title)")
        $lines.Add("")
        $lines.Add('```text')
        foreach ($noticeLine in ($noticeText -split "`r?`n")) {
            $lines.Add($noticeLine)
        }
        $lines.Add('```')
        $lines.Add("")
    }
    if ($lines.Count -gt 0 -and $lines[$lines.Count - 1] -eq "") {
        $lines.RemoveAt($lines.Count - 1)
    }

    $parent = Split-Path -Parent $outputFile
    if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent | Out-Null
    }
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllLines($outputFile, $lines, $utf8NoBom)
    Write-Host "Wrote $outputFile with $($packages.Count) dependency records."
}
finally {
    Pop-Location
}
