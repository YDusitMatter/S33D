use std::{env, fs, path::PathBuf, process::Command};

// cmake-rs 0.1.58 overwrites MSVC's Release flags when it chooses the
// generator implicitly. An explicit generator preserves CMake's /O2 defaults.
// Discover the installed generator; never hard-code a VS release or user path.
fn configure_msvc_release() {
    if !env::var("TARGET").unwrap_or_default().contains("msvc") {
        return;
    }
    let root = occt_sys::occt_path();
    let build = root.join("build");
    let cache = fs::read_to_string(build.join("CMakeCache.txt")).unwrap_or_default();
    let cache_value = |key: &str| {
        cache.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.split_once(':')?.0 == key).then(|| value.to_owned())
        })
    };
    let cmake = env::var_os("CMAKE").unwrap_or_else(|| "cmake".into());
    if env::var_os("CMAKE_GENERATOR").is_none() {
        let generator = cache_value("CMAKE_GENERATOR").or_else(|| {
            let where_exe = PathBuf::from(env::var_os("ProgramFiles(x86)")?)
                .join("Microsoft Visual Studio/Installer/vswhere.exe");
            let version = Command::new(where_exe)
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-requires",
                    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                    "-property",
                    "installationVersion",
                ])
                .output()
                .ok()?;
            let version = String::from_utf8_lossy(&version.stdout);
            let major = version.trim().split('.').next()?;
            let prefix = format!("Visual Studio {major} ");
            let output = Command::new(&cmake)
                .args(["-E", "capabilities"])
                .output()
                .ok()?;
            let capabilities: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
            capabilities["generators"]
                .as_array()?
                .iter()
                .find_map(|entry| {
                    let name = entry["name"].as_str()?;
                    name.starts_with(&prefix).then(|| name.to_owned())
                })
        });
        if let Some(generator) = generator {
            env::set_var("CMAKE_GENERATOR", generator);
        } else {
            panic!("Cannot discover CMake generator; set CMAKE_GENERATOR explicitly to preserve OCCT Release optimization");
        }
    }
    // Repair only the exact unoptimized flags emitted by the old implicit
    // generator path. Keep explicit user/toolchain flags and all build outputs.
    let mut repair = Vec::new();
    for key in ["CMAKE_C_FLAGS_RELEASE", "CMAKE_CXX_FLAGS_RELEASE"] {
        if cache_value(key).is_some_and(|flags| {
            let words: Vec<_> = flags.split_whitespace().collect();
            words == ["-nologo", "-MD", "-Brepro", "-W0"]
        }) {
            repair.push(key);
        }
    }
    if !repair.is_empty() {
        let source = cache_value("CMAKE_HOME_DIRECTORY")
            .expect("OCCT cache has flags but no source directory");
        let mut command = Command::new(&cmake);
        command.arg("-S").arg(source).arg("-B").arg(&build);
        for key in repair {
            command.arg("-U").arg(key);
        }
        let output = command.output().expect("Cannot repair OCCT Release cache");
        assert!(
            output.status.success(),
            "OCCT Release cache repair failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("cargo:warning=Restored OCCT Release optimization defaults; native libraries will rebuild");
    }
}

fn main() {
    // Honor explicit toolchain settings; otherwise discover VS's bundled CMake.
    // No developer-machine path is compiled into the application.
    if cfg!(windows) && env::var_os("CMAKE").is_none() {
        if let Some(programs) = env::var_os("ProgramFiles(x86)") {
            let where_exe =
                PathBuf::from(programs).join("Microsoft Visual Studio/Installer/vswhere.exe");
            if let Ok(output) = Command::new(where_exe)
                .args(["-latest", "-products", "*", "-property", "installationPath"])
                .output()
            {
                let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                let cmake = PathBuf::from(root)
                    .join("Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe");
                if cmake.is_file() {
                    env::set_var("CMAKE", cmake);
                }
            }
        }
    }
    // OCCT 7.8 predates CMake 4's minimum-policy removal.
    if env::var_os("CMAKE_POLICY_VERSION_MINIMUM").is_none() {
        env::set_var("CMAKE_POLICY_VERSION_MINIMUM", "3.5");
    }
    if env::var_os("CMAKE_BUILD_PARALLEL_LEVEL").is_none() {
        env::set_var("CMAKE_BUILD_PARALLEL_LEVEL", "2");
    }
    // OCCT enables unbounded MSVC /MP in each MSBuild project. _CL_ is
    // appended after command-line flags, so cap nested compiler workers too.
    if cfg!(windows) && env::var_os("_CL_").is_none() {
        env::set_var("_CL_", "/MP2");
    }
    configure_msvc_release();
    occt_sys::build_occt();
    let root = occt_sys::occt_path();
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .define("_USE_MATH_DEFINES", None)
        .define("OCCT_STATIC_BUILD", None)
        .include(root.join("include"))
        .file("native/occt_bridge.cpp")
        // Resolve Standard_Type from our OCCT-licensed lifetime fix before
        // TKernel's archive member. Do not patch the shared Cargo cache.
        .file("native/occt_patches/Standard_Type.cxx")
        .compile("s33d_occt_bridge");
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("lib").display()
    );
    for lib in [
        "TKDESTEP",
        "TKDEIGES",
        "TKDE",
        "TKXSBase",
        "TKXCAF",
        "TKVCAF",
        "TKCAF",
        "TKLCAF",
        "TKCDF",
        "TKV3d",
        "TKService",
        "TKHLR",
        "TKOffset",
        "TKFillet",
        "TKBool",
        "TKBO",
        "TKPrim",
        "TKMesh",
        "TKShHealing",
        "TKTopAlgo",
        "TKGeomAlgo",
        "TKBRep",
        "TKGeomBase",
        "TKG3d",
        "TKG2d",
        "TKMath",
        "TKernel",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    if cfg!(windows) {
        for lib in ["user32", "advapi32", "gdi32", "ws2_32", "psapi"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    println!("cargo:rerun-if-changed=native/occt_bridge.cpp");
    println!("cargo:rerun-if-changed=native/occt_patches/Standard_Type.cxx");
    println!("cargo:rerun-if-env-changed=CMAKE");
    println!("cargo:rerun-if-env-changed=CMAKE_GENERATOR");
}
