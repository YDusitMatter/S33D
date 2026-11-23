# Exact geometry build and distribution notes

Implementation status: 2026-09-09. Local upgrade implemented; workspace regression tests passed (235 tests, normal process exit) and the Debug workspace builds. New panels still require manual visual acceptance; this is not a public binary release. See Architecture.md for the reviewed current architecture.

S33D retains its existing App / Model / Kernel separation. The Kernel includes a small exception-catching C++ bridge to OCCT 7.8.1, supplied as source by pinned `occt-sys = 0.6.0`. No OCCT installation at a developer-specific absolute path is required. Rust owns copied B-Rep text and display meshes; no native pointer is persisted in projects.

## Building

Install Rust and a supported native C++ toolchain. On Windows use Visual Studio Build Tools with the C++ workload and CMake component. `crates/kernel/build.rs` honors an existing `CMAKE`; otherwise it locates Visual Studio's CMake through vswhere. CMake policy compatibility is set for OCCT's bundled build. The first build compiles OCCT and can take many minutes. Subsequent builds reuse `target/OCCT`.

Use `cargo build --workspace -j 2` or `cargo test --workspace -j 2`. Offline builds additionally require the locked Cargo packages to be present in the local cache. Native build parallelism defaults to two to avoid saturating desktop memory. Explicit existing environment settings are respected. Windows has been exercised locally; other operating systems still require native build verification.

### MSVC optimization correction (2026-09-12)

The old implicit-generator path in cmake-rs 0.1.58 replaced Release compiler flags without restoring `/O2`. A Release directory name alone did not establish an optimized OCCT build. The project now supplies an explicit discovered generator: an existing OCCT cache takes precedence, otherwise the installed Visual Studio C++ toolchain is matched against CMake's generator capabilities. An explicit `CMAKE_GENERATOR` remains supported.

For the exact known faulty cached C/C++ Release flags (`-nologo -MD -Brepro -W0`), the build script unsets only those two cache entries and lets CMake restore its defaults. Other cached settings, user-customized flags and native source files are not deleted or patched. This causes a one-time native recompilation; it can take tens of minutes. Merely rebuilding the Rust executable with `--release` is not sufficient proof that old native libraries were corrected.

Verify both `target/OCCT/build/CMakeCache.txt` and the actual Release compiler command records under `target/OCCT/build/src/*/*.dir/Release/*.tlog/`. The default MSVC build should include `/O2 /Ob2` and `NDEBUG`; custom toolchains require their own verification. Correct flags are not a measured speedup or a substitute for geometry regression tests.

## Persistence and boundaries

Project version 2 stores exact B-Rep alongside a disposable display mesh, including geometry referenced by undo/redo. Version 1 remains readable but mesh-only data does not become an original smooth surface. Invalid stored B-Rep fails loading rather than silently falling back to the mesh.

STEP imports preserve native surfaces. Exact STEP exports preserve native geometry. Legacy polygonal Boolean tools and STEP exports are sewn into planar B-Rep faces; this is not reconstruction of a lost analytic surface. Legacy conversion requires a closed manifold and at most 100,000 triangles, and reports failure instead of emitting invalid disconnected shells. Curve equations remain editable independent of their sampled display points; project reopening rebuilds samples.

Offset uses a frozen input snapshot and selected topology indices. Background previews do not commit until Done. Cancellation drops the result receiver; it does not forcibly interrupt OCCT. Very complex native operations may still consume resources until they finish. The native mutex serializes OCCT calls. Native exception handling is not process isolation against native memory faults.

## Binary release gate

### OCCT compatibility patch

`crates/kernel/native/occt_patches/Standard_Type.cxx` is derived from OCCT 7.8.1 and retains its upstream LGPL/exception copyright header (it is not relicensed as MPL). A small marked change gives the type registry container process lifetime. STEP static handles can otherwise unregister types after the ordinary registry destructor, causing a reproducible access violation during native test process exit. Individual type descriptors still unregister and free normally; the registry container is reclaimed by the OS at process exit. The bridge archive resolves this implementation before TKernel's original member, without modifying a shared Cargo cache. Include this modified source with corresponding OCCT source/build materials.

OCCT is not covered by the project's MPL-2.0 license. Ship the upstream LGPL-2.1 and OCCT exception texts in `assets/licenses/occt/`, the third-party inventory, and all other required notices. The current development build uses static libraries. Before distributing binaries, review the applicable source/relinking requirements and provide the corresponding build materials; notices alone are not a complete distribution compliance package. Do not describe the OCCT header exception as a blanket static-link exemption. No public release is performed by this upgrade.

### MSVC distribution path mapping

First resolve the OCCT directory actually selected by this build. Read the
kernel build-script output's `cargo:rustc-link-search=native=` entry ending in
`OCCT/lib` and normalize its relative components. Inspect that directory's
`build/CMakeCache.txt` and source directory before configuring anything. A
custom `CARGO_TARGET_DIR` can select a different OCCT cache from the repository's
default `target/OCCT`; multiple caches may coexist. Back up the selected cache,
verify its source prefix is covered by the mapping, and inspect the final PE
rather than treating a successful build in another directory as evidence.

Rust path remapping does not rewrite C++ `__FILE__` strings. Before a Windows
distribution build, configure OCCT's C and C++ Release flags with `/MD /O2 /Ob2
/DNDEBUG /experimental:deterministic` and `/pathmap:<source-prefix>=<public-prefix>`.
Map the actual Cargo source cache to `/cargo-cache`, the project root to
`/workspace`, and any personal source prefix to `/build-user`. Keep source
prefixes local; do not publish a machine-specific command or CMake cache.

The configured MSVC must first pass a small preprocessing probe proving that
`__FILE__` uses the mapped prefix without an ignored-option warning. Preserve
the existing cache and libraries, configure the flags through CMake, rebuild
and install OCCT's Release libraries, then rebuild both S33D executables.
Verify actual Release compiler records and scan the final PE files for the
original project/cache/home prefixes in UTF-8 and UTF-16. Do not edit compiled
strings or disable assertions to pass the check. Native changes require fresh
geometry regressions and installer checks. Compiler support is version-specific;
probe failure blocks this path rather than silently dropping the options.

Apply the mapping to every MSVC native dependency, including libraries built
through Rust's cc crate. Scope equivalent flags to the build child process
(for example through `_CL_`), never to a global user setting. Previously built
native dependencies can retain unmapped diagnostics; invalidate the affected
package's release artifacts before relinking. `lzma-sys` assertion headers also
require the UTF-16 scan. Probe both ordinary and wide `__FILE__` expansion and
do not disable assertions or patch executable bytes to remove paths.
