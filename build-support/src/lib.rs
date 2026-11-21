//! Acquire, build, and package the pinned KosmicKrisp archive.

use anyhow::{Context, Result, ensure};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use walkdir::WalkDir;

const VERSION: &str = "26.3.0";
const REVISION: &str = "0a6fa60abb737d26ac8a316a4aad7c9ddf64f9e4";
const TOOLS_REVISION: &str = "ef96ed763b43b59b33b31b362f09a02b729fa1c9";
const HEADERS_REVISION: &str = "496543121ce6419f23d6fa5d7194ba66c36212d2";
const ARTIFACT: &str = "kosmickrisp-26.3.0-aarch64-apple-darwin-macos26.tar.gz";
const STATIC_MESON: &str = include_str!("../native/static.meson");
const STATIC_ENTRY: &str = include_str!("../native/static_entry.c");

fn baseline() -> Value {
    let mut recipe = Sha256::new();
    recipe.update(include_str!("lib.rs"));
    recipe.update(STATIC_MESON);
    recipe.update(STATIC_ENTRY);
    json!({
        "schema": 1,
        "revision": REVISION,
        "spirv_tools": TOOLS_REVISION,
        "spirv_headers": HEADERS_REVISION,
        "target": "aarch64-apple-darwin",
        "deployment_target": "26.0",
        "recipe": format!("{:x}", recipe.finalize()),
    })
}

fn validate(directory: &Path) -> Result<()> {
    let manifest: Value = serde_json::from_reader(File::open(directory.join("manifest.json"))?)?;
    for (key, value) in baseline().as_object().unwrap() {
        ensure!(
            manifest.get(key) == Some(value),
            "artifact {key} must be {value}"
        );
    }
    let mut magic = [0; 8];
    File::open(directory.join("libKosmicKrisp.a"))?.read_exact(&mut magic)?;
    ensure!(
        &magic == b"!<arch>\n",
        "libKosmicKrisp.a must be a thin static archive"
    );
    ensure!(
        directory.join("licenses").is_dir(),
        "artifact is missing licenses"
    );
    Ok(())
}

fn unpack(archive: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in tar::Archive::new(GzDecoder::new(File::open(archive)?)).entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        // Extract regular files only. Never follow links or accept parent paths.
        ensure!(
            path.components()
                .all(|part| matches!(part, Component::Normal(_) | Component::CurDir)),
            "invalid archive path: {}",
            path.display()
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "unsupported archive entry: {}",
            path.display()
        );
        ensure!(
            entry.unpack_in(destination)?,
            "invalid archive path: {}",
            path.display()
        );
    }
    validate(destination)
}

fn digest(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn download(url: &str, checksum: &str, out: &Path, destination: &Path) -> Result<()> {
    ensure!(
        checksum.len() == 64
            && checksum
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "KOSMICKRISP_ARTIFACT_SHA256 must be a lowercase SHA-256 digest"
    );
    ensure!(
        url.starts_with("https://"),
        "artifact downloads require HTTPS"
    );
    let cached = out.join(format!("{checksum}.tar.gz"));
    if !cached.exists() {
        let mut temporary = tempfile::NamedTempFile::new_in(out)?;
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .new_agent();
        let mut response = agent.get(url).call()?;
        io::copy(&mut response.body_mut().as_reader(), &mut temporary)?;
        temporary.persist(&cached)?;
    }
    if digest(&cached)? != checksum {
        fs::remove_file(cached)?;
        anyhow::bail!("artifact checksum mismatch");
    }
    unpack(&cached, destination)
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("could not run {command:?}"))?;
    ensure!(status.success(), "{command:?} failed: {status}");
    Ok(())
}

fn output(command: &mut Command) -> Result<String> {
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("could not run {command:?}"))?;
    ensure!(
        output.status.success(),
        "{command:?} failed: {}",
        output.status
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn checkout(repository: &str, revision: &str, source: &Path) -> Result<()> {
    if !source.join(".git").exists() {
        fs::create_dir_all(source)?;
        run(Command::new("git").arg("init").arg(source))?;
        run(Command::new("git")
            .args(["fetch", "--depth=1", repository, revision])
            .current_dir(source))?;
        run(Command::new("git")
            .args(["checkout", "--detach", "FETCH_HEAD"])
            .current_dir(source))?;
    }
    ensure!(
        output(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(source)
        )? == revision,
        "{} checkout does not match the pinned revision",
        source.display()
    );
    Ok(())
}

fn build(work: &Path, destination: &Path) -> Result<()> {
    ensure!(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "source builds require ARM64 macOS with Xcode"
    );
    fs::create_dir_all(work)?;
    let source = work.join("mesa");
    let tools = work.join("SPIRV-Tools");
    let headers = tools.join("external/spirv-headers");
    checkout(
        "https://gitlab.freedesktop.org/mesa/mesa.git",
        REVISION,
        &source,
    )?;
    checkout(
        "https://github.com/KhronosGroup/SPIRV-Tools.git",
        TOOLS_REVISION,
        &tools,
    )?;
    checkout(
        "https://github.com/KhronosGroup/SPIRV-Headers.git",
        HEADERS_REVISION,
        &headers,
    )?;

    let prefix = work.join("prefix");
    let tools_build = work.join("tools-build");
    run(Command::new("cmake")
        .arg("-S")
        .arg(&tools)
        .arg("-B")
        .arg(&tools_build)
        .args([
            "-G",
            "Ninja",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_SHARED_LIBS=OFF",
            "-DSPIRV_SKIP_TESTS=ON",
            "-DSPIRV_SKIP_EXECUTABLES=ON",
            "-DCMAKE_OSX_ARCHITECTURES=arm64",
            "-DCMAKE_OSX_DEPLOYMENT_TARGET=26.0",
        ])
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display())))?;
    let jobs = std::thread::available_parallelism()?.get().to_string();
    run(Command::new("cmake")
        .arg("--build")
        .arg(&tools_build)
        .args(["--parallel", &jobs]))?;
    run(Command::new("cmake").arg("--install").arg(&tools_build))?;

    let driver = source.join("src/kosmickrisp/vulkan");
    fs::write(driver.join("static_entry.c"), STATIC_ENTRY)?;
    let meson_file = driver.join("meson.build");
    let meson = fs::read_to_string(&meson_file)?;
    if !meson.contains("'kosmickrisp_static'") {
        fs::write(&meson_file, meson + STATIC_MESON)?;
    }
    // Darwin needs weak imports for entry points that the driver does not implement.
    let generator = source.join("src/vulkan/util/vk_entrypoints_gen.py");
    let generated = fs::read_to_string(&generator)?;
    fs::write(&generator, generated.replace(
        "#if !defined(_MSC_VER) && defined(VK_ENTRY_USE_WEAK)",
        "#if defined(__APPLE__) && defined(VK_ENTRY_USE_WEAK)\n#define VK_ENTRY_WEAK __attribute__ ((weak_import))\n#elif !defined(_MSC_VER) && defined(VK_ENTRY_USE_WEAK)"))?;

    let mesa_build = work.join("mesa-build");
    let mut setup = Command::new("meson");
    setup.arg("setup");
    if mesa_build.join("build.ninja").exists() {
        setup.arg("--reconfigure");
    }
    setup.arg(&mesa_build).arg(&source).args([
        "--buildtype=release",
        "--prefer-static",
        "-Db_lto=false",
        "-Dplatforms=macos",
        "-Dvulkan-drivers=kosmickrisp",
        "-Dgallium-drivers=",
        "-Dopengl=false",
        "-Degl=disabled",
        "-Dglx=disabled",
        "-Dzstd=disabled",
        "-Dzlib=disabled",
        "-Dshader-cache=disabled",
        "-Dxmlconfig=disabled",
        "-Dexpat=disabled",
        "-Dlibunwind=disabled",
        "-Dbuild-tests=false",
        "-Dshared-llvm=enabled",
    ]);
    let mut pkg_paths = vec![prefix.join("lib/pkgconfig")];
    if let Some(paths) = env::var_os("PKG_CONFIG_PATH") {
        pkg_paths.extend(env::split_paths(&paths));
    }
    setup.env("PKG_CONFIG_PATH", env::join_paths(pkg_paths)?);
    setup.env("MACOSX_DEPLOYMENT_TARGET", "26.0");
    run(&mut setup)?;
    run(Command::new("meson")
        .arg("compile")
        .arg("-C")
        .arg(&mesa_build)
        .args(["-j", &jobs, "kosmickrisp_static"]))?;
    run(Command::new("xcrun")
        .args(["libtool", "-static", "-o"])
        .arg(destination.join("libKosmicKrisp.a"))
        .arg(mesa_build.join("src/kosmickrisp/vulkan/libkosmickrisp_static.a"))
        .arg(prefix.join("lib/libSPIRV-Tools.a")))?;
    let architectures = output(
        Command::new("xcrun")
            .args(["lipo", "-archs"])
            .arg(destination.join("libKosmicKrisp.a")),
    )?;
    ensure!(
        architectures == "arm64",
        "expected arm64 only, found {architectures}"
    );

    let licenses = destination.join("licenses");
    for root in [&source, &tools, &headers] {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| e.file_name() != ".git")
        {
            let entry = entry?;
            if entry.file_type().is_file()
                && ["LICENSE", "COPYING", "NOTICE"]
                    .iter()
                    .any(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
            {
                let target = licenses
                    .join(root.file_name().unwrap())
                    .join(entry.path().strip_prefix(root)?);
                fs::create_dir_all(target.parent().unwrap())?;
                fs::copy(entry.path(), target)?;
            }
        }
    }
    let mut manifest = baseline();
    manifest["version"] = json!(VERSION);
    manifest["xcode"] = json!(output(Command::new("xcodebuild").arg("-version"))?);
    manifest["sdk"] = json!(output(Command::new("xcrun").args([
        "--sdk",
        "macosx",
        "--show-sdk-version"
    ]))?);
    fs::write(
        destination.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    validate(destination)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let target = destination.join(entry.path().strip_prefix(source)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else {
            ensure!(
                entry.file_type().is_file(),
                "unsupported artifact entry: {}",
                entry.path().display()
            );
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Prepare an artifact from source, a local directory, or a verified download.
/// Explicit source mode takes precedence over artifact environment variables.
pub fn prepare(out: &Path, source: bool) -> Result<PathBuf> {
    fs::create_dir_all(out)?;
    let out = fs::canonicalize(out)?;
    let staged = tempfile::tempdir_in(&out)?;
    if source {
        build(&out.join("source"), staged.path())?;
    } else if let Some(local) = env::var_os("KOSMICKRISP_STATIC_DIR") {
        let local = Path::new(&local);
        validate(local)?;
        copy_tree(local, staged.path())?;
    } else {
        let missing = "no release is configured yet: set KOSMICKRISP_STATIC_DIR, set KOSMICKRISP_ARTIFACT_URL and KOSMICKRISP_ARTIFACT_SHA256, or enable build-kosmickrisp";
        let url = env::var("KOSMICKRISP_ARTIFACT_URL").context(missing)?;
        let checksum = env::var("KOSMICKRISP_ARTIFACT_SHA256").context(missing)?;
        download(&url, &checksum, &out, staged.path())?;
    }
    let destination = out.join("artifact");
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(staged.path(), &destination)?;
    Ok(destination)
}

/// Write a release archive and its SHA-256 sidecar from a prepared artifact.
pub fn package(artifact: &Path, out: &Path) -> Result<PathBuf> {
    validate(artifact)?;
    fs::create_dir_all(out)?;
    let archive = out.join(ARTIFACT);
    let mut bundle = tar::Builder::new(GzEncoder::new(
        File::create(&archive)?,
        Compression::default(),
    ));
    for entry in WalkDir::new(artifact).sort_by_file_name() {
        let entry = entry?;
        if entry.file_type().is_dir() {
            continue;
        }
        ensure!(
            entry.file_type().is_file(),
            "unsupported artifact entry: {}",
            entry.path().display()
        );
        bundle.append_path_with_name(entry.path(), entry.path().strip_prefix(artifact)?)?;
    }
    bundle.into_inner()?.finish()?;
    fs::write(
        out.join(format!("{ARTIFACT}.sha256")),
        format!("{}  {ARTIFACT}\n", digest(&archive)?),
    )?;
    Ok(archive)
}

#[cfg(test)]
mod tests;
