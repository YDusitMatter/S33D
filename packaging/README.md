# Windows Alpha Preview packaging

The installer is a per-user Windows x64 package for S33D Alpha Preview. It needs
no administrator rights and installs under `%LOCALAPPDATA%\Programs\S33D Alpha Preview`.
The fixed AppId identifies this preview channel across updates. The installer
offers a desktop shortcut and creates a Start menu shortcut; neither installation
nor uninstallation changes Agent registration or user settings. It never starts
S33D automatically. Close S33D and its bridge before updating.

The 2026-11-22 candidate refresh retains version `0.1.0-alpha.1`. See
`RELEASE-NOTES.md` for the latest behavior. An audited, frozen candidate commit,
local secret scanning, fresh privacy-mapped binaries and clean-machine acceptance
are release gates. Maintainer audit reports and the internal release checklist
are kept separately. The commands below describe the packagers; they do not
replace those gates. Earlier EXEs and installers must not be reused for the refresh.

## Build

Use PowerShell with the release binaries, this release's icon assets, corresponding source
archive, Visual Studio C++ tools/runtime redistributable files, and Inno Setup 6
(configured compiler version: 6.7.3). The packaging script does not build Rust, fetch
dependencies, install tools, sign the package, or install the resulting package.

```powershell
cargo build --release --locked -p s33d-app --bin s33d-app --bin s33d-agent
.\scripts\package-source.ps1 -OutputDirectory 'target\source-bundle\alpha-1'
.\scripts\package-windows.ps1 `
    -SourceBundlePath 'target\source-bundle\alpha-1\S33D-0.1.0-alpha.1-source.zip' `
    -IsccPath 'target\packaging-tools\InnoSetup\ISCC.exe'
```

`-StageOnly` assembles and checks the payload without invoking Inno Setup.
`-ReleaseDirectory`, `-DumpbinPath`, and `-RuntimeDirectory` support custom build
and tool locations. RuntimeDirectory must contain approved, redistributable x64
DLLs. The default discovery uses `vswhere` and Visual Studio's x64 CRT redist
directory. It recursively follows imported DLLs and refuses missing non-system
dependencies; no DLLs are copied from Windows directories. A clean Windows 10/11
x64 machine still needs a suitable graphics driver and its OS runtime components.
The minimum installer OS version is Windows 10 1809; this is an installation
floor, not a claim that all GPU/OS combinations have been tested.

Each run creates a new directory below `target/packages` and prints its location.
An explicit `-OutputDirectory` must not already exist. It contains `stage`, the
installer EXE, and an adjacent SHA-256 file. `stage/package-manifest.json` records
the payload hashes and dependency imports without developer-machine paths.
The script checks x64 images, the GUI's PE ProductVersion, and the bridge's
`--version`; both must match the requested version. Rebuild both together.

The installer uses `assets/icons/s33d-seed.ico`; the application executable,
Start menu, desktop and uninstall entry use `assets/icons/s33d-sprouted.ico`.
Both roles use the same transparent iron-walnut revision approved by the user
on 2026-11-12: silver S3 pipes and leaves, copper 3D text, a rounded iron shell
and roots connected to the silver 3 through standard pipe fittings. It is an
image-tool edit of the supplied design. The existing filenames preserve installer
compatibility.
The window PNG is embedded separately in the GUI. The source archive includes
the current master `assets/icons/s33d-artwork.png`, both derived PNG/ICO pairs and their
hash manifest. See `assets/icons/README.md`; regenerate with
`scripts/update-brand-icons.ps1 -Force` before rebuilding the GUI and installer.
Both ICO files, both PNGs, the master and manifest are mandatory. Use
`scripts/package-source.ps1 -ValidateOnly` to check the public file inventory and
locked OCCT source checksum before packaging. This reports missing icons; it
does not label an incomplete source kit as a finished release.

Packages produced without a code-signing certificate are unsigned. Windows may
show a publisher/security warning. Do not claim the installer is signed, disable
SmartScreen or ask users to disable their security software; signing and clean
machine testing remain separate release steps.

## Payload review

The payload is an explicit file list, never a recursive workspace copy:

- Both release executables and their required app-local CRT DLLs.
- Seven built-in plug-in manifests and their README files. The implementations
  are compiled into the GUI; `plugins.rs` requires these sibling `Plug-ins` files.
- Public manuals, Agent Control instructions, optional Agent configuration
  templates, and the repository's modeling skill for manual use.
- The current release notes and `Docs/ThreadPrinting.md` correction guide.
- S33D license, third-party inventory, font notice, OCCT license/exception,
  corresponding source ZIP, and its accompanying source/build instructions.
- This release's selected installer and application icons, installation notes,
  and generated payload manifest.

The Noto Sans SC font and egui default fonts are compiled into the executable.
Their license notices are included; the packaging script does not ship another
copy of the font or copy the user's Windows fonts. Manuals are also embedded in
the GUI and included as readable files. Configuration templates and skill files
are documentation, not automatic host configuration changes.

Project files, private DSH material, `.reasonix`, Git data, development outputs,
logs, `ui_state.json`, and backups are outside the payload list. The source ZIP
must be produced and audited separately; see `SOURCE-README.md` and the source
packaging script. The binary packager checks that the ZIP exists, not that its
source/relinking materials are complete. Static OCCT linking remains subject to
its own terms; keeping notices and source together is not a legal conclusion.

## Verification before handing off

Inspect the payload manifest and source archive, then test the installer in a
disposable per-user directory or clean Windows x64 environment. Confirm the
installed app starts outside the repository, shows the expected version and
icons, discovers all seven plug-ins, and can save/reopen a basic project. Check
the Agent bridge and manual registration separately if required. Uninstall and
confirm the installed files/shortcuts disappear while a user-created project
and `%LOCALAPPDATA%\S33D` settings remain. No installer `Run`, `Registry`,
`InstallDelete`, or `UninstallDelete` sections are used. Only the files and
shortcuts installed by Inno Setup are tracked for removal.


## Installer editions

`package-windows.ps1 -InstallLanguage en` (default) or `-InstallLanguage zh-CN` selects a single setup language and emits an edition suffix in the filename. Stage includes install-defaults.json with only default_language; it is installation-owned, never a user's ui_state.json. The desktop reads it from its executable directory only as a fallback when saved language is missing. Both editions share one AppId and preserve saved choices. Supply a new OutputDirectory for each edition.
