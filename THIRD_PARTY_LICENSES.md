# Third-Party Licenses

This inventory covers the third-party Rust packages locked by `Cargo.lock` and the fonts embedded in S33D through `epaint_default_fonts`. It was generated on 2026-11-16 by `scripts/generate-third-party-licenses.ps1`.

The license expressions below are copied from Cargo package metadata. They are an inventory, not a replacement for the complete upstream license and notice files. Distributions should ship this file together with S33D's root `LICENSE`.

## Embedded fonts

S33D enables egui's `default_fonts` feature. The following font files from `epaint_default_fonts` 0.31.1 are compiled into the executable:

| Embedded font | Upstream license | Copyright / source |
| --- | --- | --- |
| Hack Regular | MIT plus Bitstream Vera terms; DejaVu portions are public domain | Copyright 2018 Source Foundry Authors; Bitstream Vera copyright 2003 Bitstream, Inc. |
| Noto Emoji Regular | SIL Open Font License 1.1 (`OFL-1.1`) | Noto font family / Google |
| Ubuntu Light | Ubuntu Font Licence 1.0 (`Ubuntu-font-1.0`) | Ubuntu typeface |
| emoji-icon-font | MIT | Copyright 2014 John Slegers |

The full notices supplied with those fonts are reproduced in the Font license texts section below.

### Sketch outline fonts

Sketch Text uses the above Ubuntu Light and Hack fonts, plus unmodified Noto Sans SC (SIL Open Font License 1.1) embedded from `assets/fonts/NotoSansSC.ttf`. The full Noto Sans SC copyright and license are in `assets/fonts/OFL-NotoSansSC.txt`; include this notice in distributions. Drawing PDF export also embeds the same unmodified Noto Sans SC font. No Windows system fonts are used to generate exported text geometry or PDF text.

Noto Sans SC source: https://github.com/google/fonts/tree/a85815a42757630ce188fdad368c2dfc444d4773/ofl/notosanssc. SHA-256: `a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da`.

### Runtime system-font fallback (not redistributed)

On Windows, S33D may read SimHei, Microsoft YaHei, SimSun, Segoe UI, or Segoe UI Symbol from the user's local Windows Fonts directory to provide CJK and calculator glyph fallback. These system font files are not stored in this repository and are not embedded or redistributed by S33D.

## Locked Rust dependency inventory

### Native geometry dependency

Open CASCADE Technology (OCCT) 7.8.1 is built from the source bundled by occt-sys 0.6.0. It is separate from S33D's MPL-2.0 license. OCCT is licensed under LGPL-2.1 with the accompanying OCCT exception; full texts are in assets/licenses/occt/LICENSE_LGPL_21.txt and assets/licenses/occt/OCCT_LGPL_EXCEPTION.txt. Upstream: https://dev.opencascade.org/.

The current build links OCCT statically. This notice alone does not satisfy all binary distribution obligations. Before publishing binaries, review and provide the corresponding source, build/relink materials and notices required by the applicable licenses. See Docs/ExactGeometryBuild.md. The exception must not be described as a blanket exemption for static linking.
S33D includes a modified OCCT Standard_Type.cxx in crates/kernel/native/occt_patches under its original LGPL/exception terms. The marked change retains the registry container through process teardown to protect late static STEP type handles. Include the modified source in corresponding source materials.

This graph contains 503 package/version records, including target-specific transitive dependencies for supported platforms.

| Package | Version | Declared license | Upstream |
| --- | --- | --- | --- |
| ab_glyph | 0.2.32 | Apache-2.0 | <https://github.com/alexheretic/ab-glyph> |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | <https://github.com/alexheretic/ab-glyph> |
| accesskit | 0.17.1 | MIT OR Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| accesskit_atspi_common | 0.10.1 | MIT OR Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| accesskit_consumer | 0.26.0 | MIT OR Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| accesskit_macos | 0.18.1 | MIT OR Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| accesskit_unix | 0.13.1 | MIT OR Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| accesskit_windows | 0.24.1 | MIT OR Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| accesskit_winit | 0.23.1 | Apache-2.0 | <https://github.com/AccessKit/accesskit> |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | <https://github.com/oyvindln/adler2> |
| ahash | 0.8.12 | MIT OR Apache-2.0 | <https://github.com/tkaitchuck/ahash> |
| aho-corasick | 1.1.5 | Unlicense OR MIT | <https://github.com/BurntSushi/aho-corasick> |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | <https://github.com/zakarumych/allocator-api2> |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | <https://github.com/nical/android_system_properties> |
| android-activity | 0.6.1 | MIT OR Apache-2.0 | <https://github.com/rust-mobile/android-activity> |
| android-properties | 0.2.2 | MIT | <https://github.com/miklelappo/android-properties> |
| approx | 0.4.0 | Apache-2.0 | <https://github.com/brendanzab/approx> |
| arboard | 3.6.1 | MIT OR Apache-2.0 | <https://github.com/1Password/arboard> |
| array-macro | 2.1.8 | MIT OR Apache-2.0 | <https://codeberg.org/xfix/array-macro> |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | <https://github.com/bluss/arrayvec> |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | <https://github.com/psychon/as-raw-xcb-connection> |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | <https://github.com/ash-rs/ash> |
| ashpd | 0.11.1 | MIT | <https://github.com/bilelmoussaoui/ashpd> |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | <https://github.com/smol-rs/async-broadcast> |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-channel> |
| async-executor | 1.14.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-executor> |
| async-fs | 2.2.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-fs> |
| async-io | 2.6.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-io> |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-lock> |
| async-net | 2.0.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-net> |
| async-process | 2.5.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-process> |
| async-recursion | 1.1.1 | MIT OR Apache-2.0 | <https://github.com/dcchut/async-recursion> |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-signal> |
| async-task | 4.7.1 | Apache-2.0 OR MIT | <https://github.com/smol-rs/async-task> |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | <https://github.com/dtolnay/async-trait> |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | <https://github.com/smol-rs/atomic-waker> |
| atspi | 0.22.0 | Apache-2.0 OR MIT | <https://github.com/odilia-app/atspi> |
| atspi-common | 0.6.0 | Apache-2.0 OR MIT | <https://github.com/odilia-app/atspi> |
| atspi-connection | 0.6.0 | Apache-2.0 OR MIT | <https://github.com/odilia-app/atspi/> |
| atspi-proxies | 0.6.0 | Apache-2.0 OR MIT | <https://github.com/odilia-app/atspi> |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | <https://github.com/cuviper/autocfg> |
| base64 | 0.13.1 | MIT/Apache-2.0 | <https://github.com/marshallpierce/rust-base64> |
| base64 | 0.21.7 | MIT OR Apache-2.0 | <https://github.com/marshallpierce/rust-base64> |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | <https://github.com/contain-rs/bit-set> |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | <https://github.com/contain-rs/bit-vec> |
| bitflags | 1.3.2 | MIT/Apache-2.0 | <https://github.com/bitflags/bitflags> |
| bitflags | 2.13.1 | MIT OR Apache-2.0 | <https://github.com/bitflags/bitflags> |
| block | 0.1.6 | MIT | <http://github.com/SSheldon/rust-block> |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/utils> |
| block2 | 0.5.1 | MIT | <https://github.com/madsmtm/objc2> |
| block2 | 0.6.2 | MIT | <https://github.com/madsmtm/objc2> |
| blocking | 1.6.2 | Apache-2.0 OR MIT | <https://github.com/smol-rs/blocking> |
| branches | 0.3.0 | MIT | <https://github.com/fereidani/branches> |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | <https://github.com/fitzgen/bumpalo> |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | <https://github.com/Lokathor/bytemuck> |
| bytemuck_derive | 1.12.0 | Zlib OR Apache-2.0 OR MIT | <https://github.com/Lokathor/bytemuck> |
| byteorder | 1.5.0 | Unlicense OR MIT | <https://github.com/BurntSushi/byteorder> |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | <https://github.com/image-rs/byteorder-lite> |
| bytes | 1.12.1 | MIT | <https://github.com/tokio-rs/bytes> |
| calloop | 0.13.0 | MIT | <https://github.com/Smithay/calloop> |
| calloop | 0.14.4 | MIT | <https://github.com/Smithay/calloop> |
| calloop-wayland-source | 0.3.0 | MIT | <https://github.com/smithay/calloop-wayland-source> |
| calloop-wayland-source | 0.4.1 | MIT | <https://github.com/smithay/calloop-wayland-source> |
| cc | 1.4.2 | MIT OR Apache-2.0 | <https://github.com/rust-lang/cc-rs> |
| cfg_aliases | 0.2.2 | MIT | <https://github.com/katharostech/cfg_aliases> |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 | <https://github.com/rust-lang/cfg-if> |
| cgl | 0.3.2 | MIT / Apache-2.0 | <https://github.com/servo/cgl-rs> |
| cgmath | 0.18.0 | Apache-2.0 | <https://github.com/rustgd/cgmath> |
| chrono | 0.4.45 | MIT OR Apache-2.0 | <https://github.com/chronotope/chrono> |
| clipboard-win | 5.4.1 | BSL-1.0 | <https://github.com/DoumanAsh/clipboard-win> |
| clipper2-rust | 1.1.0 | BSL-1.0 | <https://github.com/larsbrubaker/clipper2-rust> |
| cmake | 0.1.58 | MIT OR Apache-2.0 | <https://github.com/rust-lang/cmake-rs> |
| codespan-reporting | 0.11.1 | Apache-2.0 | <https://github.com/brendanzab/codespan> |
| combine | 4.6.7 | MIT | <https://github.com/Marwes/combine> |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/concurrent-queue> |
| convert_case | 0.4.0 | MIT | <https://github.com/rutrum/convert-case> |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | <https://github.com/servo/core-foundation-rs> |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | <https://github.com/servo/core-foundation-rs> |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 | <https://github.com/servo/core-foundation-rs> |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | <https://github.com/servo/core-foundation-rs> |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/utils> |
| crc32fast | 1.5.0 | MIT OR Apache-2.0 | <https://github.com/srijs/rust-crc32fast> |
| crossbeam-deque | 0.8.7 | MIT OR Apache-2.0 | <https://github.com/crossbeam-rs/crossbeam> |
| crossbeam-epoch | 0.9.20 | MIT OR Apache-2.0 | <https://github.com/crossbeam-rs/crossbeam> |
| crossbeam-utils | 0.8.22 | MIT OR Apache-2.0 | <https://github.com/crossbeam-rs/crossbeam> |
| crunchy | 0.2.4 | MIT | <https://github.com/eira-fransham/crunchy> |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/traits> |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | <https://github.com/rust-windowing/cursor-icon> |
| dashu-base | 0.5.1 | MIT OR Apache-2.0 | <https://github.com/cmpute/dashu> |
| dashu-int | 0.5.1 | MIT OR Apache-2.0 | <https://github.com/cmpute/dashu> |
| dashu-ratio | 0.5.1 | MIT OR Apache-2.0 | <https://github.com/cmpute/dashu> |
| derive_more | 0.99.20 | MIT | <https://github.com/JelteF/derive_more> |
| derive-new | 0.5.9 | MIT | <https://github.com/nrc/derive-new> |
| digest | 0.10.7 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/traits> |
| dispatch | 0.2.0 | MIT | <http://github.com/SSheldon/rust-dispatch> |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | <https://github.com/madsmtm/objc2> |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | <https://github.com/yaahc/displaydoc> |
| dlib | 0.5.3 | MIT | <https://github.com/elinorbgr/dlib> |
| document-features | 0.2.12 | MIT OR Apache-2.0 | <https://github.com/slint-ui/document-features> |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 | <https://github.com/marcianx/downcast-rs> |
| dpi | 0.1.2 | Apache-2.0 AND MIT | <https://github.com/rust-windowing/winit> |
| earcut | 0.4.11 | MIT OR Apache-2.0 | <https://github.com/georust/earcut> |
| ecolor | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui> |
| eframe | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui/tree/master/crates/eframe> |
| egui | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui> |
| egui_glow | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui/tree/master/crates/egui_glow> |
| egui-wgpu | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui/tree/master/crates/egui-wgpu> |
| egui-winit | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui/tree/master/crates/egui-winit> |
| either | 1.17.0 | MIT OR Apache-2.0 | <https://github.com/rayon-rs/either> |
| emath | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui/tree/master/crates/emath> |
| endi | 1.1.1 | MIT | <https://github.com/zeenix/endi> |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | <https://github.com/meithecatte/enumflags2> |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | <https://github.com/meithecatte/enumflags2> |
| epaint | 0.31.1 | MIT OR Apache-2.0 | <https://github.com/emilk/egui/tree/master/crates/epaint> |
| epaint_default_fonts | 0.31.1 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | <https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts> |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | <https://github.com/indexmap-rs/equivalent> |
| errno | 0.3.14 | MIT OR Apache-2.0 | <https://github.com/lambda-fairy/rust-errno> |
| error-code | 3.3.2 | BSL-1.0 | <https://github.com/DoumanAsh/error-code> |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | <https://github.com/smol-rs/event-listener> |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | <https://github.com/smol-rs/event-listener-strategy> |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/fastrand> |
| fax | 0.2.7 | MIT | <https://github.com/pdf-rs/fax> |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | <https://github.com/image-rs/fdeflate> |
| find-msvc-tools | 0.1.10 | MIT OR Apache-2.0 | <https://github.com/rust-lang/cc-rs> |
| flate2 | 1.1.9 | MIT OR Apache-2.0 | <https://github.com/rust-lang/flate2-rs> |
| foldhash | 0.1.5 | Zlib | <https://github.com/orlp/foldhash> |
| foldhash | 0.2.0 | Zlib | <https://github.com/orlp/foldhash> |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | <https://github.com/sfackler/foreign-types> |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | <https://github.com/sfackler/foreign-types> |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | <https://github.com/sfackler/foreign-types> |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | <https://github.com/servo/rust-url> |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | <https://github.com/smol-rs/futures-lite> |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | <https://github.com/rust-lang/futures-rs> |
| generic-array | 0.14.7 | MIT | <https://github.com/fizyk20/generic-array.git> |
| gethostname | 1.1.0 | Apache-2.0 | <https://codeberg.org/swsnr/gethostname.rs.git> |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | <https://github.com/rust-random/getrandom> |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | <https://github.com/rust-random/getrandom> |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | <https://github.com/rust-random/getrandom> |
| gl_generator | 0.14.0 | Apache-2.0 | <https://github.com/brendanzab/gl-rs/> |
| glam | 0.29.3 | MIT OR Apache-2.0 | <https://github.com/bitshifter/glam-rs> |
| glow | 0.16.0 | MIT OR Apache-2.0 OR Zlib | <https://github.com/grovesNL/glow> |
| gltf | 1.4.1 | MIT OR Apache-2.0 | <https://github.com/gltf-rs/gltf> |
| gltf-derive | 1.4.1 | MIT OR Apache-2.0 | <https://github.com/gltf-rs/gltf> |
| gltf-json | 1.4.1 | MIT OR Apache-2.0 | <https://github.com/gltf-rs/gltf> |
| glutin | 0.32.3 | Apache-2.0 | <https://github.com/rust-windowing/glutin> |
| glutin_egl_sys | 0.7.1 | Apache-2.0 | <https://github.com/rust-windowing/glutin> |
| glutin_glx_sys | 0.6.1 | Apache-2.0 | <https://github.com/rust-windowing/glutin> |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | <https://github.com/rust-windowing/glutin> |
| glutin-winit | 0.5.0 | MIT | <https://github.com/rust-windowing/glutin> |
| gpu-alloc | 0.6.2 | MIT OR Apache-2.0 | <https://github.com/zakarumych/gpu-alloc> |
| gpu-alloc-types | 0.3.1 | MIT OR Apache-2.0 | <https://github.com/zakarumych/gpu-alloc> |
| gpu-allocator | 0.27.0 | MIT OR Apache-2.0 | <https://github.com/Traverse-Research/gpu-allocator> |
| gpu-descriptor | 0.3.2 | MIT OR Apache-2.0 | <https://github.com/zakarumych/gpu-descriptor> |
| gpu-descriptor-types | 0.2.0 | MIT OR Apache-2.0 | <https://github.com/zakarumych/gpu-descriptor> |
| half | 2.7.1 | MIT OR Apache-2.0 | <https://github.com/VoidStarKat/half-rs> |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | <https://github.com/rust-lang/hashbrown> |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | <https://github.com/rust-lang/hashbrown> |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | <https://github.com/rust-lang/hashbrown> |
| heck | 0.5.0 | MIT OR Apache-2.0 | <https://github.com/withoutboats/heck> |
| hermit-abi | 0.5.2 | MIT OR Apache-2.0 | <https://github.com/hermit-os/hermit-rs> |
| hex | 0.4.3 | MIT OR Apache-2.0 | <https://github.com/KokaKiwi/rust-hex> |
| hexf-parse | 0.2.1 | CC0-1.0 | <https://github.com/lifthrasiir/hexf> |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | <https://github.com/strawlab/iana-time-zone> |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | <https://github.com/strawlab/iana-time-zone> |
| icu_collections | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| icu_locale_core | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| icu_normalizer | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| icu_normalizer_data | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| icu_properties | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| icu_properties_data | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| icu_provider | 2.2.0 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| idna | 1.1.0 | MIT OR Apache-2.0 | <https://github.com/servo/rust-url/> |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | <https://github.com/hsivonen/idna_adapter> |
| image | 0.25.10 | MIT OR Apache-2.0 | <https://github.com/image-rs/image> |
| immutable-chunkmap | 2.1.3 | Apache-2.0 OR MIT | <https://github.com/estokes/immutable-chunkmap> |
| indexmap | 2.14.0 | Apache-2.0 OR MIT | <https://github.com/indexmap-rs/indexmap> |
| inflections | 1.1.1 | MIT | <https://docs.rs/inflections> |
| Inflector | 0.11.4 | BSD-2-Clause | <https://github.com/whatisinternet/inflector> |
| itertools | 0.10.5 | MIT/Apache-2.0 | <https://github.com/rust-itertools/itertools> |
| itertools | 0.13.0 | MIT OR Apache-2.0 | <https://github.com/rust-itertools/itertools> |
| itoa | 1.0.18 | MIT OR Apache-2.0 | <https://github.com/dtolnay/itoa> |
| jni | 0.22.4 | MIT OR Apache-2.0 | <https://github.com/jni-rs/jni-rs> |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | <https://github.com/jni-rs/jni-rs> |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | <https://github.com/jni-rs/jni-sys> |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | <https://github.com/jni-rs/jni-sys> |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | <https://github.com/jni-rs/jni-sys> |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | <https://github.com/rust-lang/jobserver-rs> |
| js-sys | 0.3.104 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys> |
| katexit | 0.1.5 | MIT OR Apache-2.0 | <https://github.com/termoshtt/katexit> |
| khronos_api | 3.1.0 | Apache-2.0 | <https://github.com/brendanzab/gl-rs/> |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 | <https://github.com/timothee-haudebourg/khronos-egl> |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 | <https://github.com/rust-lang-nursery/lazy-static.rs> |
| libc | 0.2.189 | MIT OR Apache-2.0 | <https://github.com/rust-lang/libc> |
| libloading | 0.8.9 | ISC | <https://github.com/nagisa/rust_libloading/> |
| libredox | 0.1.19 | MIT | <https://gitlab.redox-os.org/redox-os/libredox.git> |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/sunfishcode/linux-raw-sys> |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/sunfishcode/linux-raw-sys> |
| litemap | 0.8.2 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| litrs | 1.0.0 | MIT OR Apache-2.0 | <https://github.com/LukasKalbertodt/litrs> |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | <https://github.com/Amanieu/parking_lot> |
| log | 0.4.33 | MIT OR Apache-2.0 | <https://github.com/rust-lang/log> |
| lz4_flex | 0.7.5 | MIT | <https://github.com/pseitz/lz4_flex> |
| lzma-sys | 0.1.20 | MIT/Apache-2.0 | <https://github.com/alexcrichton/xz2-rs> |
| malloc_buf | 0.0.6 | MIT | <https://github.com/SSheldon/malloc_buf> |
| manifold-rust | 0.13.1 | Apache-2.0 | <https://github.com/larsbrubaker/manifold-rust> |
| matext4cgmath | 0.1.0 | Apache-2.0 | <https://github.com/ricosjp/matext4cgmath> |
| memchr | 1.0.2 | Unlicense/MIT | <https://github.com/BurntSushi/rust-memchr> |
| memchr | 2.8.3 | Unlicense OR MIT | <https://github.com/BurntSushi/memchr> |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | <https://github.com/RazrFalcon/memmap2-rs> |
| memoffset | 0.9.1 | MIT | <https://github.com/Gilnaa/memoffset> |
| metal | 0.31.0 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/metal-rs> |
| minimal-lexical | 0.2.1 | MIT/Apache-2.0 | <https://github.com/Alexhuszagh/minimal-lexical> |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | <https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide> |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | <https://github.com/awxkee/moxcms.git> |
| naga | 24.0.0 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/wgpu/tree/trunk/naga> |
| ndk | 0.9.0 | MIT OR Apache-2.0 | <https://github.com/rust-mobile/ndk> |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | <https://github.com/rust-windowing/android-ndk-rs> |
| ndk-sys | 0.5.0+25.2.9519653 | MIT OR Apache-2.0 | <https://github.com/rust-mobile/ndk> |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | <https://github.com/rust-mobile/ndk> |
| nix | 0.29.0 | MIT | <https://github.com/nix-rust/nix> |
| nohash-hasher | 0.2.0 | Apache-2.0 OR MIT | <https://github.com/paritytech/nohash-hasher> |
| nom | 3.2.1 | MIT | <https://github.com/Geal/nom> |
| nom | 7.1.3 | MIT | <https://github.com/Geal/nom> |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | <https://github.com/illicitonion/num_enum> |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | <https://github.com/illicitonion/num_enum> |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | <https://github.com/rust-num/num-complex> |
| num-derive | 0.3.3 | MIT OR Apache-2.0 | <https://github.com/rust-num/num-derive> |
| num-modular | 0.6.5 | Apache-2.0 | <https://github.com/cmpute/num-modular> |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | <https://github.com/rust-num/num-traits> |
| objc | 0.2.7 | MIT | <http://github.com/SSheldon/rust-objc> |
| objc-sys | 0.3.5 | MIT | <https://github.com/madsmtm/objc2> |
| objc2 | 0.5.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2 | 0.6.4 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-app-kit | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | <https://github.com/madsmtm/objc2> |
| objc2-cloud-kit | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-contacts | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-core-data | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | <https://github.com/madsmtm/objc2> |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | <https://github.com/madsmtm/objc2> |
| objc2-core-image | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-core-location | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-encode | 4.1.0 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-foundation | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-foundation | 0.3.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | <https://github.com/madsmtm/objc2> |
| objc2-link-presentation | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-metal | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-quartz-core | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-symbols | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-ui-kit | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-uniform-type-identifiers | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| objc2-user-notifications | 0.2.2 | MIT | <https://github.com/madsmtm/objc2> |
| occt-sys | 0.6.0 | LGPL-2.1 | <https://github.com/bschwind/opencascade-rs> |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | <https://github.com/matklad/once_cell> |
| orbclient | 0.3.55 | MIT | <https://gitlab.redox-os.org/redox-os/orbclient> |
| ordered-float | 4.6.0 | MIT | <https://github.com/reem/rust-ordered-float> |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | <https://github.com/danieldg/ordered-stream> |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | <https://github.com/alexheretic/owned-ttf-parser> |
| parking | 2.2.1 | Apache-2.0 OR MIT | <https://github.com/smol-rs/parking> |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | <https://github.com/Amanieu/parking_lot> |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | <https://github.com/Amanieu/parking_lot> |
| paste | 1.0.15 | MIT OR Apache-2.0 | <https://github.com/dtolnay/paste> |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | <https://github.com/servo/rust-url/> |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | <https://github.com/taiki-e/pin-project> |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | <https://github.com/taiki-e/pin-project> |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | <https://github.com/taiki-e/pin-project-lite> |
| piper | 0.2.5 | MIT OR Apache-2.0 | <https://github.com/smol-rs/piper> |
| pkg-config | 0.3.33 | MIT OR Apache-2.0 | <https://github.com/rust-lang/pkg-config-rs> |
| plain | 0.2.3 | MIT/Apache-2.0 | <https://github.com/randomites/plain> |
| png | 0.18.1 | MIT OR Apache-2.0 | <https://github.com/image-rs/image-png> |
| polling | 3.11.0 | Apache-2.0 OR MIT | <https://github.com/smol-rs/polling> |
| pollster | 0.4.0 | Apache-2.0/MIT | <https://github.com/zesterer/pollster> |
| potential_utf | 0.1.5 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | <https://github.com/cryptocorrosion/cryptocorrosion> |
| presser | 0.3.1 | MIT OR Apache-2.0 | <https://github.com/EmbarkStudios/presser> |
| proc-macro-crate | 1.3.1 | MIT OR Apache-2.0 | <https://github.com/bkchr/proc-macro-crate> |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | <https://github.com/bkchr/proc-macro-crate> |
| proc-macro-error | 1.0.4 | MIT OR Apache-2.0 | <https://gitlab.com/CreepySkeleton/proc-macro-error> |
| proc-macro-error-attr | 1.0.4 | MIT OR Apache-2.0 | <https://gitlab.com/CreepySkeleton/proc-macro-error> |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | <https://github.com/dtolnay/proc-macro2> |
| profiling | 1.0.18 | MIT OR Apache-2.0 | <https://github.com/aclysma/profiling> |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | <https://github.com/awxkee/pxfm> |
| quick-error | 2.0.1 | MIT/Apache-2.0 | <http://github.com/tailhook/quick-error> |
| quick-xml | 0.22.0 | MIT | <https://github.com/tafia/quick-xml> |
| quick-xml | 0.30.0 | MIT | <https://github.com/tafia/quick-xml> |
| quick-xml | 0.41.0 | MIT | <https://github.com/tafia/quick-xml> |
| quote | 1.0.47 | MIT OR Apache-2.0 | <https://github.com/dtolnay/quote> |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | <https://github.com/r-efi/r-efi> |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | <https://github.com/r-efi/r-efi> |
| rand | 0.8.7 | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| rand | 0.9.5 | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | <https://github.com/rust-random/rand> |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/range-alloc> |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | <https://github.com/rust-windowing/raw-window-handle> |
| rayon | 1.12.0 | MIT OR Apache-2.0 | <https://github.com/rayon-rs/rayon> |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 | <https://github.com/rayon-rs/rayon> |
| rclite | 0.2.8 | MIT OR Apache-2.0 | <https://github.com/fereidani/rclite> |
| redox_syscall | 0.4.1 | MIT | <https://gitlab.redox-os.org/redox-os/syscall> |
| redox_syscall | 0.5.18 | MIT | <https://gitlab.redox-os.org/redox-os/syscall> |
| redox_syscall | 0.9.2 | MIT | <https://gitlab.redox-os.org/redox-os/syscall> |
| regex | 1.13.1 | MIT OR Apache-2.0 | <https://github.com/rust-lang/regex> |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | <https://github.com/rust-lang/regex> |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | <https://github.com/rust-lang/regex> |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | <https://github.com/ebkalderon/renderdoc-rs> |
| rfd | 0.15.4 | MIT | <https://github.com/PolyMeilex/rfd> |
| robust | 1.2.0 | MIT OR Apache-2.0 | <https://github.com/georust/robust> |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | <https://github.com/djc/rustc-version-rs> |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | <https://github.com/rust-lang-nursery/rustc-hash> |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | <https://github.com/rust-lang/rustc-hash> |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/bytecodealliance/rustix> |
| rustix | 1.1.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/bytecodealliance/rustix> |
| ruststep | 0.4.0 | Apache-2.0 | <https://github.com/ricosjp/ruststep> |
| ruststep-derive | 0.4.0 | Apache-2.0 | <https://github.com/ricosjp/ruststep> |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | <https://github.com/dtolnay/rustversion> |
| same-file | 1.0.6 | Unlicense/MIT | <https://github.com/BurntSushi/same-file> |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 | <https://github.com/alexcrichton/scoped-tls> |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | <https://github.com/bluss/scopeguard> |
| semver | 1.0.28 | MIT OR Apache-2.0 | <https://github.com/dtolnay/semver> |
| serde | 1.0.229 | MIT OR Apache-2.0 | <https://github.com/serde-rs/serde> |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | <https://github.com/serde-rs/serde> |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | <https://github.com/serde-rs/serde> |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | <https://github.com/serde-rs/json> |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | <https://github.com/dtolnay/serde-repr> |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/hashes> |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/hashes> |
| shlex | 2.0.1 | MIT OR Apache-2.0 | <https://github.com/comex/rust-shlex> |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | <https://github.com/vorner/signal-hook> |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | <https://github.com/seancroach/simd_cesu8> |
| simd-adler32 | 0.3.10 | MIT | <https://github.com/mcountryman/simd-adler32> |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | <https://github.com/rusticstuff/simdutf8> |
| slab | 0.4.12 | MIT | <https://github.com/tokio-rs/slab> |
| slotmap | 1.1.1 | Zlib | <https://github.com/orlp/slotmap> |
| smallvec | 1.15.2 | MIT OR Apache-2.0 | <https://github.com/servo/rust-smallvec> |
| smithay-client-toolkit | 0.19.2 | MIT | <https://github.com/smithay/client-toolkit> |
| smithay-client-toolkit | 0.20.0 | MIT | <https://github.com/smithay/client-toolkit> |
| smithay-clipboard | 0.7.3 | MIT | <https://github.com/smithay/smithay-clipboard> |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | <https://github.com/rust-analyzer/smol_str> |
| spade | 2.15.1 | MIT OR Apache-2.0 | <https://github.com/Stoeoef/spade> |
| spirv | 0.3.0+sdk-1.3.268.0 | Apache-2.0 | <https://github.com/gfx-rs/rspirv> |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | <https://github.com/storyyeller/stable_deref_trait> |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | <https://github.com/nvzqz/static-assertions-rs> |
| strum | 0.26.3 | MIT | <https://github.com/Peternator7/strum> |
| strum_macros | 0.26.4 | MIT | <https://github.com/Peternator7/strum> |
| syn | 1.0.109 | MIT OR Apache-2.0 | <https://github.com/dtolnay/syn> |
| syn | 2.0.119 | MIT OR Apache-2.0 | <https://github.com/dtolnay/syn> |
| syn | 3.0.3 | MIT OR Apache-2.0 | <https://github.com/dtolnay/syn> |
| synstructure | 0.13.2 | MIT | <https://github.com/mystor/synstructure> |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | <https://github.com/Stebalien/tempfile> |
| termcolor | 1.4.1 | Unlicense OR MIT | <https://github.com/BurntSushi/termcolor> |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | <https://github.com/dtolnay/thiserror> |
| thiserror | 2.0.20 | MIT OR Apache-2.0 | <https://github.com/dtolnay/thiserror> |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | <https://github.com/dtolnay/thiserror> |
| thiserror-impl | 2.0.20 | MIT OR Apache-2.0 | <https://github.com/dtolnay/thiserror> |
| tiff | 0.11.3 | MIT | <https://github.com/image-rs/image-tiff> |
| tinystr | 0.8.3 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| toml_datetime | 0.6.11 | MIT OR Apache-2.0 | <https://github.com/toml-rs/toml> |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | <https://github.com/toml-rs/toml> |
| toml_edit | 0.19.15 | MIT OR Apache-2.0 | <https://github.com/toml-rs/toml> |
| toml_edit | 0.25.13+spec-1.1.0 | MIT OR Apache-2.0 | <https://github.com/toml-rs/toml> |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | <https://github.com/toml-rs/toml> |
| tracing | 0.1.44 | MIT | <https://github.com/tokio-rs/tracing> |
| tracing-attributes | 0.1.31 | MIT | <https://github.com/tokio-rs/tracing> |
| tracing-core | 0.1.36 | MIT | <https://github.com/tokio-rs/tracing> |
| truck-base | 0.5.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-derivers | 0.1.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-geometry | 0.5.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-geotrait | 0.4.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-meshalgo | 0.4.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-modeling | 0.6.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-polymesh | 0.6.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-stepio | 0.3.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| truck-topology | 0.6.0 | Apache-2.0 | <https://github.com/ricosjp/truck> |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | <https://github.com/harfbuzz/ttf-parser> |
| type-map | 0.5.1 | MIT/Apache-2.0 | <https://github.com/kardeiz/type-map> |
| typenum | 1.20.1 | MIT OR Apache-2.0 | <https://github.com/paholg/typenum> |
| uds_windows | 1.2.1 | MIT | <https://github.com/haraldh/rust_uds_windows> |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | <https://github.com/dtolnay/unicode-ident> |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | <https://github.com/unicode-rs/unicode-segmentation> |
| unicode-width | 0.1.14 | MIT OR Apache-2.0 | <https://github.com/unicode-rs/unicode-width> |
| unicode-xid | 0.2.6 | MIT OR Apache-2.0 | <https://github.com/unicode-rs/unicode-xid> |
| url | 2.5.8 | MIT OR Apache-2.0 | <https://github.com/servo/rust-url> |
| urlencoding | 2.1.3 | MIT | <https://github.com/kornelski/rust_urlencoding> |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | <https://github.com/hsivonen/utf8_iter> |
| uuid | 1.24.0 | Apache-2.0 OR MIT | <https://github.com/uuid-rs/uuid> |
| version_check | 0.9.5 | MIT/Apache-2.0 | <https://github.com/SergioBenitez/version_check> |
| vtkio | 0.6.3 | MIT OR Apache-2.0 | <https://github.com/elrnv/vtkio> |
| walkdir | 2.5.0 | Unlicense/MIT | <https://github.com/BurntSushi/walkdir> |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/bytecodealliance/wasi> |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/bytecodealliance/wasi-rs> |
| wasm-bindgen | 0.2.127 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen> |
| wasm-bindgen-futures | 0.4.77 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures> |
| wasm-bindgen-macro | 0.2.127 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro> |
| wasm-bindgen-macro-support | 0.2.127 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro-support> |
| wasm-bindgen-shared | 0.2.127 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared> |
| wayland-backend | 0.3.16 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-client | 0.31.15 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-csd-frame | 0.3.0 | MIT | <https://github.com/rust-windowing/wayland-csd-frame> |
| wayland-cursor | 0.31.14 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-protocols | 0.32.13 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-protocols-experimental | 20250721.0.1 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-protocols-misc | 0.3.12 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-protocols-plasma | 0.3.12 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-protocols-wlr | 0.3.12 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-scanner | 0.31.11 | MIT | <https://github.com/smithay/wayland-rs> |
| wayland-sys | 0.31.11 | MIT | <https://github.com/smithay/wayland-rs> |
| web-sys | 0.3.104 | MIT OR Apache-2.0 | <https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys> |
| web-time | 1.1.0 | MIT OR Apache-2.0 | <https://github.com/daxpedda/web-time> |
| webbrowser | 1.2.4 | MIT OR Apache-2.0 | <https://github.com/amodm/webbrowser-rs> |
| weezl | 0.1.12 | MIT OR Apache-2.0 | <https://github.com/image-rs/weezl> |
| wgpu | 24.0.5 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/wgpu> |
| wgpu-core | 24.0.5 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/wgpu> |
| wgpu-hal | 24.0.4 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/wgpu> |
| wgpu-types | 24.0.0 | MIT OR Apache-2.0 | <https://github.com/gfx-rs/wgpu> |
| winapi | 0.3.9 | MIT/Apache-2.0 | <https://github.com/retep998/winapi-rs> |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | <https://github.com/retep998/winapi-rs> |
| winapi-util | 0.1.11 | Unlicense OR MIT | <https://github.com/BurntSushi/winapi-util> |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | <https://github.com/retep998/winapi-rs> |
| windows | 0.58.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_aarch64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_aarch64_msvc | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_i686_gnu | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_i686_gnullvm | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_i686_msvc | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_x86_64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-core | 0.58.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-implement | 0.58.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-interface | 0.58.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-result | 0.2.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-strings | 0.1.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | <https://github.com/microsoft/windows-rs> |
| winit | 0.30.13 | Apache-2.0 | <https://github.com/rust-windowing/winit> |
| winnow | 0.5.40 | MIT | <https://github.com/winnow-rs/winnow> |
| winnow | 1.0.4 | MIT | <https://github.com/winnow-rs/winnow> |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | <https://github.com/bytecodealliance/wit-bindgen> |
| writeable | 0.6.3 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| x11-dl | 2.21.0 | MIT | <https://github.com/AltF02/x11-rs.git> |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | <https://github.com/psychon/x11rb> |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | <https://github.com/psychon/x11rb> |
| xcursor | 0.3.11 | MIT | <https://github.com/esposm03/xcursor-rs> |
| xdg-home | 1.3.0 | MIT | <https://github.com/zeenix/xdg-home> |
| xkbcommon-dl | 0.4.2 | MIT | <https://github.com/rust-windowing/xkbcommon-dl> |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | <https://github.com/notgull/xkeysym> |
| xml-rs | 0.8.29 | MIT | <https://github.com/kornelski/xml-rs> |
| xz2 | 0.1.7 | MIT/Apache-2.0 | <https://github.com/alexcrichton/xz2-rs> |
| yoke | 0.8.3 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| yoke-derive | 0.8.2 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| zbus | 4.4.0 | MIT | <https://github.com/dbus2/zbus/> |
| zbus | 5.19.0 | MIT | <https://github.com/z-galaxy/zbus/> |
| zbus_macros | 4.4.0 | MIT | <https://github.com/dbus2/zbus/> |
| zbus_macros | 5.19.0 | MIT | <https://github.com/z-galaxy/zbus/> |
| zbus_names | 3.0.0 | MIT | <https://github.com/dbus2/zbus/> |
| zbus_names | 4.3.4 | MIT | <https://github.com/z-galaxy/zbus/> |
| zbus_xml | 4.0.0 | MIT | <https://github.com/dbus2/zbus/> |
| zbus-lockstep | 0.4.4 | MIT | <https://github.com/luukvanderduim/zbus-lockstep> |
| zbus-lockstep-macros | 0.4.4 | MIT | <https://github.com/luukvanderduim/zbus-lockstep> |
| zcheapstr | 1.1.0 | MIT | <https://github.com/z-galaxy/zcheapstr/> |
| zerocopy | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT | <https://github.com/google/zerocopy> |
| zerocopy-derive | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT | <https://github.com/google/zerocopy> |
| zerofrom | 0.1.8 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| zerofrom-derive | 0.1.7 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| zerotrie | 0.2.4 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| zerovec | 0.11.6 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| zerovec-derive | 0.11.3 | Unicode-3.0 | <https://github.com/unicode-org/icu4x> |
| zmij | 1.0.23 | MIT | <https://github.com/dtolnay/zmij> |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | <https://github.com/etemesi254/zune-image> |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | <https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg> |
| zvariant | 4.2.0 | MIT | <https://github.com/dbus2/zbus/> |
| zvariant | 5.14.0 | MIT | <https://github.com/z-galaxy/zbus/> |
| zvariant_derive | 4.2.0 | MIT | <https://github.com/dbus2/zbus/> |
| zvariant_derive | 5.14.0 | MIT | <https://github.com/z-galaxy/zbus/> |
| zvariant_utils | 2.1.0 | MIT | <https://github.com/dbus2/zbus/> |
| zvariant_utils | 4.0.0 | MIT | <https://github.com/z-galaxy/zbus/> |

All locked third-party package records declare a license expression in Cargo metadata.

## Font license texts

### Hack Regular

```text
The work in the Hack project is Copyright 2018 Source Foundry Authors and licensed under the MIT License

The work in the DejaVu project was committed to the public domain.

Bitstream Vera Sans Mono Copyright 2003 Bitstream Inc. and licensed under the Bitstream Vera License with Reserved Font Names "Bitstream" and "Vera"
MIT License

Copyright (c) 2018 Source Foundry Authors

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
BITSTREAM VERA LICENSE

Copyright (c) 2003 by Bitstream, Inc. All Rights Reserved. Bitstream Vera is a trademark of Bitstream, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy of the fonts accompanying this license ("Fonts") and associated documentation files (the "Font Software"), to reproduce and distribute the Font Software, including without limitation the rights to use, copy, merge, publish, distribute, and/or sell copies of the Font Software, and to permit persons to whom the Font Software is furnished to do so, subject to the following conditions:

The above copyright and trademark notices and this permission notice shall be included in all copies of one or more of the Font Software typefaces.

The Font Software may be modified, altered, or added to, and in particular the designs of glyphs or characters in the Fonts may be modified and additional glyphs or characters may be added to the Fonts, only if the fonts are renamed to names not containing either the words "Bitstream" or the word "Vera".

This License becomes null and void to the extent applicable to Fonts or Font Software that has been modified and is distributed under the "Bitstream Vera" names.

The Font Software may be sold as part of a larger software package but no copy of one or more of the Font Software typefaces may be sold by itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL BITSTREAM OR THE GNOME FOUNDATION BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER DEALINGS IN THE FONT SOFTWARE.

Except as contained in this notice, the names of Gnome, the Gnome Foundation, and Bitstream Inc., shall not be used in advertising or otherwise to promote the sale, use or other dealings in this Font Software without prior written authorization from the Gnome Foundation or Bitstream Inc., respectively. For further information, contact: fonts at gnome dot org.
```

### Noto Emoji Regular — SIL Open Font License 1.1

```text
This Font Software is licensed under the SIL Open Font License,
Version 1.1.

This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font
creation efforts of academic and linguistic communities, and to
provide a free and open framework in which fonts may be shared and
improved in partnership with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply to
any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software
components as distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to,
deleting, or substituting -- in part or in whole -- any of the
components of the Original Version, by changing formats or by porting
the Font Software to a new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed,
modify, redistribute, and sell modified and unmodified copies of the
Font Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components, in
Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the
corresponding Copyright Holder. This restriction only applies to the
primary font name as presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created using
the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

### Ubuntu Light — Ubuntu Font Licence 1.0

```text
-------------------------------
UBUNTU FONT LICENCE Version 1.0
-------------------------------

PREAMBLE
This licence allows the licensed fonts to be used, studied, modified and
redistributed freely. The fonts, including any derivative works, can be
bundled, embedded, and redistributed provided the terms of this licence
are met. The fonts and derivatives, however, cannot be released under
any other licence. The requirement for fonts to remain under this
licence does not require any document created using the fonts or their
derivatives to be published under this licence, as long as the primary
purpose of the document is not to be a vehicle for the distribution of
the fonts.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this licence and clearly marked as such. This may
include source files, build scripts and documentation.

"Original Version" refers to the collection of Font Software components
as received under this licence.

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to
a new environment.

"Copyright Holder(s)" refers to all individuals and companies who have a
copyright ownership of the Font Software.

"Substantially Changed" refers to Modified Versions which can be easily
identified as dissimilar to the Font Software by users of the Font
Software comparing the Original Version with the Modified Version.

To "Propagate" a work means to do anything with it that, without
permission, would make you directly or secondarily liable for
infringement under applicable copyright law, except executing it on a
computer or modifying a private copy. Propagation includes copying,
distribution (with or without modification and with or without charging
a redistribution fee), making available to the public, and in some
countries other activities as well.

PERMISSION & CONDITIONS
This licence does not grant any rights under trademark law and all such
rights are reserved.

Permission is hereby granted, free of charge, to any person obtaining a
copy of the Font Software, to propagate the Font Software, subject to
the below conditions:

1) Each copy of the Font Software must contain the above copyright
notice and this licence. These can be included either as stand-alone
text files, human-readable headers or in the appropriate machine-
readable metadata fields within text or binary files as long as those
fields can be easily viewed by the user.

2) The font name complies with the following:
(a) The Original Version must retain its name, unmodified.
(b) Modified Versions which are Substantially Changed must be renamed to
avoid use of the name of the Original Version or similar names entirely.
(c) Modified Versions which are not Substantially Changed must be
renamed to both (i) retain the name of the Original Version and (ii) add
additional naming elements to distinguish the Modified Version from the
Original Version. The name of such Modified Versions must be the name of
the Original Version, with "derivative X" where X represents the name of
the new work, appended to that name.

3) The name(s) of the Copyright Holder(s) and any contributor to the
Font Software shall not be used to promote, endorse or advertise any
Modified Version, except (i) as required by this licence, (ii) to
acknowledge the contribution(s) of the Copyright Holder(s) or (iii) with
their explicit written permission.

4) The Font Software, modified or unmodified, in part or in whole, must
be distributed entirely under this licence, and must not be distributed
under any other licence. The requirement for fonts to remain under this
licence does not affect any document created using the Font Software,
except any version of the Font Software extracted from a document
created using the Font Software may only be distributed under this
licence.

TERMINATION
This licence becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER
DEALINGS IN THE FONT SOFTWARE.
```

### emoji-icon-font — MIT License

```text
MIT License

Copyright (c) 2014 John Slegers

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```
