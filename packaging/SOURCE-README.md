# Corresponding sources / 对应源码

S33D Alpha Preview is authored by YDZM and keeps its MPL-2.0 project license.
OCCT, Rust dependencies and fonts retain their separate licenses. This source
package and the accompanying notices are build materials, not a legal opinion
that every distribution obligation has been independently audited.

S33D 作者为 YDZM，项目保持 MPL-2.0。OCCT、Rust 依赖及字体分别遵循自己的许可。
本包提供对应源码和重编译材料，不表示已完成所有发行法律义务的独立审核。

## Contents / 内容

- `S33D/`: the public application, model and kernel sources, Cargo.lock, build
  scripts, icons, fonts, plug-in manifests, public documentation and notices.
- `third-party/occt-sys-0.6.0.crate`: the complete published dependency archive
  checked against Cargo.lock. It contains the OCCT 7.8.1 sources supplied by this
  dependency, its build scripts and license. It is a gzip-compressed tar archive,
  not a compiled library and not a copy of the entire upstream Git repository.
- `source-manifest.json`: archive checksums and the S33D version.
- S33D's OCCT compatibility modification is in
  `S33D/crates/kernel/native/occt_patches/Standard_Type.cxx`; it keeps its upstream
  license header. Read `S33D/Docs/ExactGeometryBuild.md` before rebuilding.

源码包不含用户模型、Git 数据、日志、同事目录或个人配置。完整锁定的 OCCT 依赖
源码以 `.crate` 原始归档保存（可用 Windows 的 `tar` 解压），项目补丁单独保留。

Version `0.1.0-alpha.1` uses the same transparent high-contrast artwork for the
installer and application. At the user's request, the supplied design was
edited with the built-in image tool to enlarge the S3 lettering and leaves and
strengthen their colors. The `s33d-seed` and `s33d-sprouted` filenames retain
their existing roles. The current master is
`S33D/assets/icons/s33d-artwork.png`; from the extracted `S33D` directory, run
`scripts/update-brand-icons.ps1 -Force` to reproduce the PNG/ICO assets and hash
manifest. See `S33D/assets/icons/README.md` for the sizes and verification steps.

`0.1.0-alpha.1` 的安装器与主程序统一使用用户于 2026-11-11 要求增强的透明图案：
内置图像工具放大 S3 和叶片并提高色彩对比。当前母版、派生资源、校验清单与
转换脚本均包含在源码包内；旧文件名保留兼容，原始附件与旧版留在本地备份。

## Rebuild / replace the OCCT library

Use Windows x64 with Rust, Visual Studio C++ Build Tools, Windows SDK and CMake.
Other Cargo.lock dependencies are fetched from their recorded registry sources;
the source kit does not vendor every Rust crate. `--offline` only works after
those dependencies have been cached. No code-signing key is required to build
or run a replacement executable. S33D does not require a signature check or
activation service for modified builds.

Extract this ZIP into a new writable folder. In PowerShell, from that folder:

```powershell
New-Item -ItemType Directory -Path third-party/expanded
tar -xf third-party/occt-sys-0.6.0.crate -C third-party/expanded
$localOcct = (Resolve-Path third-party/expanded/occt-sys-0.6.0).Path.Replace('\','/')
$patchSetting = 'patch.crates-io.occt-sys.path="' + $localOcct + '"'
cargo build --manifest-path S33D/Cargo.toml --release --config $patchSetting -j 2
```

Modify the extracted `third-party/expanded/occt-sys-0.6.0/OCCT` sources before
the final build to compile/link a replacement OCCT into S33D. Cargo's local path
patch changes the OCCT entry in the copied Cargo.lock; therefore this command
does not use `--locked`. It does not modify the shared Cargo registry cache.
For another revision, start from a fresh copy of the source kit and a fresh
build output directory so old native libraries cannot be reused by mistake.

编译替换库时，先修改解压后的 OCCT 源码，再运行上述本地路径 patch 构建。此操作
只修改当前源码副本的锁文件，不修改全局 Cargo 缓存；建议每次替换库使用新的
源码副本及构建目录。输出为 `S33D/target/release/s33d-app.exe` 与
`s33d-agent.exe`；若设置了 `CARGO_TARGET_DIR`，以该变量指定的目录为准。

The first native build can take tens of minutes. Keep the plug-in manifests and
other installed runtime files beside rebuilt executables. Close S33D before
replacing executables. To produce an installer for your rebuild, see
`S33D/packaging/README.md`. Packaging still requires both installer/application
ICO files, PNG inputs, the supplied master and hash manifest. Rebuild the GUI
after changing the assets so its executable and window icons match the installer.

Visual C++ redistributable DLLs are separate Microsoft components. They are not
covered by MPL or the OCCT license. Obtain them from a licensed Visual Studio
redistributable directory or the official Microsoft redistributable installer;
do not redistribute arbitrary DLLs copied from a Windows system directory.
