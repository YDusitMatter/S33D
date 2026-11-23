use std::{env, fs, path::PathBuf, process::Command};

#[path = "src/identity.rs"]
mod identity;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(s33d_brand_icon)");
    for path in [
        "src",
        "../kernel",
        "../model",
        "../../Docs/UserManual.en.md",
        "../../Docs/UserManual.zh-CN.md",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=S33D_BUILD_ID");
    let build_id = env::var("S33D_BUILD_ID").unwrap_or_else(|_| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()
    });
    assert!(
        !build_id.is_empty()
            && build_id.len() <= 32
            && build_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "Invalid S33D_BUILD_ID"
    );
    println!("cargo:rustc-env=S33D_BUILD_ID={build_id}");
    for (name, fallback) in [
        ("S33D_BUILD_TIME", "development"),
        ("S33D_BUILD_CHANNEL", "development"),
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        let value = env::var(name).unwrap_or_else(|_| fallback.to_owned());
        assert!(
            !value.is_empty()
                && value.len() <= 40
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-: .TZ".contains(&b)),
            "Invalid build metadata"
        );
        println!("cargo:rustc-env={name}={value}");
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icons = manifest.join("../../assets/icons");
    let png = icons.join("s33d-sprouted.png");
    let ico = icons.join("s33d-sprouted.ico");
    println!("cargo:rerun-if-changed={}", png.display());
    println!("cargo:rerun-if-changed={}", ico.display());
    if !png.is_file() || !ico.is_file() {
        println!(
            "cargo:warning=S33D Alpha branding is incomplete: assets/icons/s33d-sprouted.png and .ico are required. Building with the existing default window icon and without Windows branding resources; do not package this build."
        );
        return;
    }
    println!("cargo:rustc-cfg=s33d_brand_icon");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::copy(ico, output.join("s33d.ico")).expect("copy S33D executable icon");
    let numeric_version = format!(
        "{},{},{},0",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    );
    let flags = 8 | if env!("CARGO_PKG_VERSION_PRE").is_empty() {
        0
    } else {
        2
    };
    let resource_text = format!(
        r#"#pragma code_page(65001)
LANGUAGE 0x09, 0x01
1 ICON "s33d.ico"
1 VERSIONINFO
FILEVERSION {numeric_version}
PRODUCTVERSION {numeric_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS {flags}
FILEOS 0x00040004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "{author}\0"
            VALUE "FileDescription", "{product}\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "s33d-app\0"
            VALUE "OriginalFilename", "s33d-app.exe\0"
            VALUE "ProductName", "{product}\0"
            VALUE "ProductVersion", "{version}\0"
            VALUE "PrivateBuild", "{build_id}\0"
            VALUE "Comments", "Project license: {license}. Third-party licenses are provided separately.\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#,
        product = identity::PRODUCT_NAME,
        version = identity::VERSION,
        author = identity::AUTHOR,
        license = identity::LICENSE,
    );
    fs::write(output.join("s33d.rc"), resource_text).expect("write S33D Windows resource script");

    println!("cargo:rerun-if-env-changed=RC");
    let (mut compiler, resource) = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let resource = output.join("s33d.res");
        let mut compiler = Command::new(find_rc());
        compiler.args(["/nologo", "/fo"]).arg(&resource);
        (compiler, resource)
    } else {
        let resource = output.join("s33d-resource.o");
        let mut compiler = Command::new(env::var_os("RC").unwrap_or_else(|| "windres".into()));
        compiler.args(["-O", "coff", "-o"]).arg(&resource);
        (compiler, resource)
    };
    let result = compiler
        .current_dir(&output)
        .arg("s33d.rc")
        .output()
        .expect(
            "run Windows resource compiler; install Windows SDK or set RC to its compiler path",
        );
    assert!(
        result.status.success(),
        "Windows resource compilation failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("cargo:rustc-link-arg-bin=s33d-app={}", resource.display());
}

fn find_rc() -> PathBuf {
    if let Some(compiler) = env::var_os("RC") {
        return compiler.into();
    }
    println!("cargo:rerun-if-env-changed=PATH");
    if let Some(path) = env::var_os("PATH") {
        if let Some(compiler) = env::split_paths(&path)
            .map(|entry| entry.join("rc.exe"))
            .find(|entry| entry.is_file())
        {
            return compiler;
        }
    }
    let host = env::var("HOST").unwrap_or_default();
    let architecture = if host.starts_with("aarch64") {
        "arm64"
    } else if host.starts_with("x86_64") {
        "x64"
    } else {
        "x86"
    };
    for variable in ["WindowsSdkVerBinPath", "WindowsSdkDir", "ProgramFiles(x86)"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let mut candidates = Vec::new();
    if let Some(directory) = env::var_os("WindowsSdkVerBinPath") {
        candidates.push(PathBuf::from(directory).join(architecture).join("rc.exe"));
    }
    let sdk_root = env::var_os("WindowsSdkDir").map(PathBuf::from).or_else(|| {
        env::var_os("ProgramFiles(x86)")
            .map(|directory| PathBuf::from(directory).join("Windows Kits/10"))
    });
    if let Some(sdk_root) = sdk_root {
        let sdk_bin = sdk_root.join("bin");
        if let Ok(entries) = fs::read_dir(&sdk_bin) {
            let mut versions: Vec<_> = entries
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("10."))
                .map(|entry| entry.path())
                .collect();
            versions.sort_by_cached_key(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .split('.')
                    .map(|part| part.parse::<u32>().unwrap_or(0))
                    .collect::<Vec<_>>()
            });
            candidates.extend(
                versions
                    .into_iter()
                    .rev()
                    .map(|version| version.join(architecture).join("rc.exe")),
            );
        }
        candidates.push(sdk_bin.join(architecture).join("rc.exe"));
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .expect("Windows SDK rc.exe was not found; install Windows SDK or set RC to rc.exe")
}
