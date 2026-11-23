# Linux Alpha Preview 测试包

目标：Ubuntu 24.04 x86_64，版本 `0.1.0-alpha.1`。这是原生 Linux 构建流程；Windows EXE 不可作为 Linux 程序分发。其他发行版、旧版 glibc 和显卡驱动必须另行验收。

## 安装形式

交付为包含原生程序和安装脚本的 `S33D-*-linux-x64-local-test-*.tar.gz`。解压后，在普通桌面用户下运行 `./install.sh`，需要 Python 3.11 或更新版本。不要用 sudo。

安装到 `~/.local/opt/s33d-alpha-preview-test-日期`，创建同名 `~/.local/bin` 启动链接和用户开始菜单项。日期不同的测试版可以并存。不会自动启动程序、注册 MCP、关联文件或安装系统软件。

卸载命令由安装脚本打印，格式为 `python3 <安装目录>/install.py --uninstall`。只删除清单记录且没有修改的文件及本次快捷方式，保留用户工程、共享配置和未知/修改后的文件。Linux 设置为 `$XDG_CONFIG_HOME/s33d/ui_state.json`，或 `~/.config/s33d/ui_state.json`；测试版仍共享这些设置。

## 原生构建

需要 Rust/Cargo、rustfmt、GCC/G++、CMake、pkg-config、Python 3.11+、X11/Wayland 开发库及本项目锁定依赖。参考 eframe 的[原生 Linux 构建说明](https://github.com/emilk/egui/tree/0.31.1/crates/eframe)。初次编译还会从匹配的 occt-sys crate 源码编译 OCCT。

从隐私审阅且格式检查通过的源码副本执行：

```bash
export CARGO_HOME=/opt/s33d-build/cargo
export RUSTUP_HOME=/opt/s33d-build/rustup
export PATH="$CARGO_HOME/bin:$PATH"
bash scripts/build-linux-test.sh /opt/s33d-build/run-NEW /opt/s33d-build/S33D-0.1.0-alpha.1-source.zip YYYY-MM-DD
```

运行目录必须不存在。脚本检查格式，重建主程序和 Agent，执行完整 Release 工作区测试，之后才打包。自定义 Cargo 输出目录会提前放入七个插件资源，供测试使用。包内插件始终与主程序相邻；Agent 也与主程序相邻。编译使用路径映射，日志放入私有目录。

源码 ZIP 必须与实际构建源码逐项一致，并包含匹配的 OCCT crate、修改后的桥接/补丁、许可证及构建资料；禁止复制整个工作区。尚未批准的 Architecture 文档和内部记录继续排除。平台测试不代替正式发布的 Git/身份/签名审批。

## 运行和验收

`package-linux.py` 拒绝非 Linux/x86_64、非 ELF 文件、错误 Agent 版本、缺失的动态依赖以及残留的本机源码/用户路径，并核对对应源码归档。安装包的哈希清单和归档内容另行逐项核对、进行隐私扫描。

目标机器需要兼容的 glibc/libstdc++、X11 或 Wayland 桌面，以及适合 wgpu 的图形驱动。动态加载的 X11/Wayland、Vulkan/EGL、文件选择器 portal 等组件不能仅依靠 ldd 验证。常用 Ubuntu 运行组件包括 `libx11-6`、`libxrandr2`、`libxi6`、`libxcursor1`、`libxkbcommon0`、`libxkbcommon-x11-0`、`libwayland-client0`、`libegl1`、`libvulkan1` 和适合桌面的 `xdg-desktop-portal` 后端。不要把开发机的 GPU 驱动或系统字体随包复制。

必须记录实际 ELF/依赖闭包、完整测试、安装/启动/Agent 查询/卸载结果及最终包的隐私扫描。WSL 或 Xvfb 的软件渲染检查不能冒充用户另一台 Linux 电脑的物理显示器、显卡或格式样本验收。未完成原生构建时，脚本或源码 ZIP 只称为构建材料，不称为可安装程序。

桌面快捷方式遵循 [Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry/latest-single/)。本地测试包不签名，也不自动上传或发布。

## 渲染格式与测试环境

3D 渲染管线必须使用 eframe 提供的实际画面格式，不能固定为 BGRA。部分 Linux OpenGL 后端使用 RGBA；实体、线框与选中覆盖层应使用同一格式。除了完整测试，还要在实际 wgpu adapter 上显式运行格式读回回归，并在安装后的程序中创建实体验证：

```bash
cargo test --release --locked --offline -p s33d-app --bin s33d-app render3d::tests::gpu_viewport_accepts_rgba_and_bgra_targets -- --ignored --exact
```

WSL 1 可用于编译，但其套接字行为与真实 Linux 内核有差异，不能靠跳过端口占用测试完成验收。使用真实 Linux 内核运行完整测试；软件虚拟机负载较高时可加 `-- --test-threads=1` 串行执行全部用例。软件虚拟机和 Mesa 软件渲染仍不能代替实际显卡与物理显示器验收。

球壳精确拉伸测试的异步等待默认仍为 90 秒。已证实软件 CPU 仿真的耗时限制时，可显式设置 `S33D_TEST_FACE_COMMIT_TIMEOUT_SECONDS=300` 后执行完整测试；允许范围为 90–300 秒。此参数仅在测试辅助函数内使用，不改变程序行为、几何断言或正常测试默认值。必须保留原失败记录，在验收报告中注明实际等待参数；不能把它报告为已通过原 90 秒软件仿真时限。
