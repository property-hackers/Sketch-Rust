use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(apex_component)");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    compile_icon();
    compile_apex_component();
}

/// Finds a resource compiler: `RC`, the Windows SDK `rc.exe` (Windows host),
/// or `llvm-rc` on PATH (cross builds).
fn find_resource_compiler() -> Option<PathBuf> {
    if let Some(rc) = env::var_os("RC").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(rc);
    }
    if cfg!(windows) {
        if let Some(rc) = find_sdk_rc() {
            return Some(rc);
        }
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .flat_map(|dir| [dir.join("llvm-rc"), dir.join("llvm-rc.exe")])
        .find(|p| p.is_file())
}

fn find_sdk_rc() -> Option<PathBuf> {
    let kits = PathBuf::from(env::var_os("ProgramFiles(x86)")?).join("Windows Kits/10/bin");
    let host = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "x86"
    };
    let mut versions: Vec<_> = fs::read_dir(&kits)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    versions.sort();
    versions.reverse();
    versions
        .into_iter()
        .map(|p| p.join(host).join("rc.exe"))
        .find(|p| p.is_file())
        .or_else(|| {
            let p = kits.join(host).join("rc.exe");
            p.is_file().then_some(p)
        })
}

fn compile_icon() {
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/app-icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=RC");
    let Some(rc) = find_resource_compiler() else {
        // A Windows host keeps the strict behaviour; cross builds skip the icon.
        assert!(
            !cfg!(windows),
            "No resource compiler (rc.exe from the Windows SDK, or RC) found"
        );
        println!("cargo:warning=No resource compiler (rc.exe, llvm-rc or RC) found; building without the app icon");
        return;
    };
    // Rebuild when the compiler found changes (instead of watching all of PATH).
    println!("cargo:rerun-if-changed={}", rc.display());
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let resource = out.join("app-icon.rc");
    let compiled = out.join("app-icon.res");
    fs::write(
        &resource,
        format!(
            "1 ICON \"{}\"\n",
            icon.display().to_string().replace('\\', "/")
        ),
    )
    .expect("write icon resource");
    let is_llvm = rc
        .file_stem()
        .is_some_and(|n| n.to_string_lossy().starts_with("llvm-rc"));
    let mut command = Command::new(&rc);
    if is_llvm {
        command.arg("/FO").arg(&compiled);
    } else {
        command.arg("/nologo").arg("/fo").arg(&compiled);
    }
    let status = command.arg(&resource).status();
    if !status.as_ref().is_ok_and(|s| s.success()) {
        // A Windows host with the SDK keeps the strict behaviour; cross
        // builds with llvm-rc/RC just lose the icon.
        assert!(
            !cfg!(windows),
            "Windows icon resource compilation failed: {status:?}"
        );
        println!(
            "cargo:warning=Icon resource compilation with {} failed ({status:?}); building without the app icon",
            rc.display()
        );
        return;
    }
    println!("cargo:rustc-link-arg-bin=sketch-app={}", compiled.display());
}

fn compile_apex_component() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/apex_compat.cs");
    let legacy = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/apex_legacy.cs");
    println!("cargo:rerun-if-changed={}", legacy.display());
    let geometry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/apex_geometry.cs");
    println!("cargo:rerun-if-changed={}", geometry.display());
    let drafts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/apex_drafts.cs");
    println!("cargo:rerun-if-changed={}", drafts.display());
    let codes = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/apex_codes.cs");
    println!("cargo:rerun-if-changed={}", codes.display());
    let codec = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/apex_file.cs");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", codec.display());
    println!("cargo:rerun-if-env-changed=APEX_INTEGRATION_SDK");
    println!("cargo:rerun-if-env-changed=SKETCH_REQUIRE_APEX");
    let require = env::var("SKETCH_REQUIRE_APEX").as_deref() == Ok("1");
    let skip = |reason: &str| {
        if require {
            panic!("SKETCH_REQUIRE_APEX=1 but the Apex compatibility component cannot be built: {reason}");
        }
        println!("cargo:warning=Apex compatibility component not built: {reason}");
    };
    if !cfg!(windows) {
        skip("it needs a Windows host");
        return;
    }
    let system = PathBuf::from(env::var_os("SystemRoot").expect("SystemRoot"));
    println!(
        "cargo:rerun-if-changed={}",
        system.join("assembly/GAC_MSIL").display()
    );
    if let Some(explicit) = env::var_os("APEX_INTEGRATION_SDK") {
        assert!(
            PathBuf::from(&explicit).is_file(),
            "APEX_INTEGRATION_SDK does not point to Apex.Integration.DotNet.dll: {}",
            PathBuf::from(&explicit).display()
        );
    }
    let sdk = env::var_os("APEX_INTEGRATION_SDK")
        .map(PathBuf::from)
        .or_else(|| {
            let gac = system.join("assembly/GAC_MSIL/Apex.Integration.DotNet");
            let mut versions: Vec<_> = fs::read_dir(gac)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect();
            versions.sort();
            versions.reverse();
            versions
                .into_iter()
                .map(|p| p.join("Apex.Integration.DotNet.dll"))
                .find(|p| p.is_file())
        })
        .filter(|p| p.is_file());
    let Some(sdk) = sdk else {
        skip("set APEX_INTEGRATION_SDK to the installed Apex.Integration.DotNet.dll");
        return;
    };
    println!("cargo:rerun-if-changed={}", sdk.display());
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("apex-compat.dll");
    let status = Command::new(system.join("Microsoft.NET/Framework64/v4.0.30319/csc.exe"))
        .args([
            "/nologo",
            "/target:winexe",
            "/optimize+",
            "/reference:System.Windows.Forms.dll",
            "/reference:System.Drawing.dll",
            "/reference:System.Web.Extensions.dll",
        ])
        .arg(format!("/out:{}", output.display()))
        .arg(format!("/reference:{}", sdk.display()))
        .arg(format!(
            "/resource:{},SketchRust.HostContract",
            sdk.display()
        ))
        .arg(source.to_string_lossy().replace('/', "\\"))
        .arg(geometry.to_string_lossy().replace('/', "\\"))
        .arg(drafts.to_string_lossy().replace('/', "\\"))
        .arg(codes.to_string_lossy().replace('/', "\\"))
        .arg(codec.to_string_lossy().replace('/', "\\"))
        .arg(legacy.to_string_lossy().replace('/', "\\"))
        .status()
        .expect("run .NET Framework component compiler");
    assert!(
        status.success(),
        "Compile embedded Apex compatibility component"
    );
    // ExecuteAssembly must have an entry point without allocating a console.
    let bytes = fs::read(&output).expect("read embedded host component");
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    assert_eq!(
        u16::from_le_bytes(bytes[pe + 92..pe + 94].try_into().unwrap()),
        2,
        "The embedded host component must use the Windows GUI subsystem"
    );
    println!("cargo:rustc-cfg=apex_component");
}
