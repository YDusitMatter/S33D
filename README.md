# S33D

S33D is a local, offline-first 3D CAD application written in Rust. It is currently a developer preview aimed at personal design, 3D-printing, and small-workshop workflows.

S33D 是一款使用 Rust 开发、以本地离线工作为主的 3D CAD 应用。目前项目处于开发者预览阶段，主要面向个人设计、3D 打印和小型制造工作流。

## Current capabilities

- 2D Sketch with line, arc, circle, rectangle, and polygon tools
- Extrude, Pocket, Revolve, Boolean, Mirror, Split, Array, Chamfer, and Fillet
- Parametric Draw 3D primitives and a gear-generator plugin
- Edge, surface, region, and multi-object selection
- STEP, STL, OBJ, PLY, and GLB import/export; IGES and 3MF build-geometry import
- A single File > Export format selector, remembered input/output directories, and Edit Copy/Cut/Paste
- `.s33d` project save/load with undo/redo history
- Local Agent Control for Codex, Claude Code, and scripts through MCP/JSON
- Section view with a draggable plane and live equation; non-intersecting objects remain whole
- Numeric tool entry focuses the first parameter once; Extrude/Pocket dragging updates the same depth
- Measurement, configurable units, and English/Simplified Chinese UI; Graphite Studio is the default for new preferences
- Cylindrical-wall diameter editing and thread generation, including certified Sketch Extrude/Pocket bores; Correct resizes the bore/blank to the specified nominal thread while retaining thread parameters
- A borderless startup image (at least two seconds) and workspace sized to the current Windows work area/DPI, background loading/modeling progress, and independently selectable uncut bands after partial threading
- Scrollable short-window instructions and generator parameters with accessible footer actions, stable camera controls, clipped texture previews, and a separate thread-preset save window

## Architecture

The workspace contains three Rust crates:

- `s33d-kernel`: geometry abstraction, truck B-rep operations, manifold-rust mesh Boolean operations, tessellation, and file I/O
- `s33d-model`: documents, features, sketches, regions, and undo/redo data
- `s33d-app`: eframe/egui desktop UI and the custom wgpu viewport

Internal geometry uses millimeters and a Y-up coordinate system. STEP conversion is handled at the file boundary. See [the user manual](Docs/UserManual.md) and [exact geometry build notes](Docs/ExactGeometryBuild.md) for more detail.

## Build and run

Install a current stable Rust toolchain and the native build tools required by your platform, then run:

```powershell
cargo run --release -p s33d-app --bin s33d-app
```

Run the complete test suite with:

```powershell
cargo test --workspace --locked
```

Windows is the primary development platform. The architecture keeps the UI and geometry layers portable, but other desktop platforms have not yet received the same level of testing.

## Documentation

- [English user manual](Docs/UserManual.en.md)
- [简体中文用户手册](Docs/UserManual.zh-CN.md)
- [Agent Control for Codex, Claude Code, and scripts](Docs/AgentControl.md)
- [User manual](Docs/UserManual.md)
- [Thread printing and nominal-diameter correction](Docs/ThreadPrinting.md)

## License

S33D source code is licensed under the [Mozilla Public License 2.0](LICENSE). Third-party Rust dependencies and embedded-font notices are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).


Attach photos in your existing visual Agent chat: the Agent understands them, calls S33D to model and compares native model images to correct the result. S33D photo import is optional project reference storage or alternate delivery. See [Photo to model](Docs/PhotoToModel.md) for connection steps and an example. The shipped MCP bridge includes the workflow; no separate personal skill, Python or desktop capture dependency is required.
