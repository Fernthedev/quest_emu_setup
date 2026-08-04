// # macOS / Linux path pattern
// <path-to-ndk>/toolchains/llvm/prebuilt/<host-os>/lib/clang/<version>/lib/linux/aarch64/lldb-server

// # Windows path pattern
// <path-to-ndk>\toolchains\llvm\prebuilt\windows-x86_64\lib\clang\<version>\lib\linux\aarch64\lldb-server

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::Duration,
};

use color_eyre::eyre::{Context, ContextCompat, bail};

use crate::{
    commands::Command,
    constants::{self, adb_path},
};

#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[derive(Debug)]
pub struct DebugArgs {
    #[cfg_attr(feature = "clap", command(subcommand))]
    action: DebugAction,
}

#[cfg_attr(feature = "clap", derive(clap::Subcommand))]
#[derive(Debug)]
pub enum DebugAction {
    /// Push the NDK's lldb-server binary onto the device and copy it into the
    /// target app's private storage. Does not start it; use `start-lldb-server`
    /// for that. The app must be debuggable (`android:debuggable="true"`).
    InstallLldbServer {
        /// Android package name of the app to debug, e.g. "com.beatgames.beatsaber"
        package: String,

        /// The architecture of the target device (e.g. "aarch64", "x86_64").
        /// Auto-detected from the device via `adb` if omitted.
        arch: Option<String>,
    },

    /// Launch the lldb-server already installed (see `install-lldb-server`) in the
    /// target app's private storage as a debug platform server via `run-as`, and
    /// forward the debug port to the host.
    StartLldbServer {
        /// Android package name of the app to debug, e.g. "com.beatgames.beatsaber"
        package: String,

        /// The port to forward and listen on
        #[cfg_attr(feature = "clap", arg(short, long, default_value_t = 5039))]
        port: u16,
    },

    /// Start lldb-server (see `start-lldb-server`), force-stop and relaunch the app,
    /// then open a VS Code attach session for it via the lldb-dap extension
    /// (llvm-vs-code-extensions.lldb-dap).
    Attach {
        /// Android package name of the app to debug, e.g. "com.beatgames.beatsaber"
        package: String,

        /// The architecture of the target device (e.g. "aarch64", "x86_64").
        /// Auto-detected from the device via `adb` if omitted.
        arch: Option<String>,

        /// The port to forward and listen on
        #[cfg_attr(feature = "clap", arg(short, long, default_value_t = 5039))]
        port: u16,

        /// Path to the VS Code executable used to open the attach session (default: "code")
        #[cfg_attr(feature = "clap", arg(long, default_value_t = "code".to_string()))]
        code_bin: String,

        /// Launch VS Code with the lldb-dap attach URI. By default the URI is only
        /// printed, since it can also be opened manually or on another machine.
        #[cfg_attr(feature = "clap", arg(long, default_value_t = false))]
        open: bool,

        /// .so files (or directories to search for .so files in) with debug symbols
        /// to preload in lldb (`target symbols add`)
        #[cfg_attr(feature = "clap", arg(long = "symbols"))]
        symbols: Vec<PathBuf>,
    },
}

impl Command for DebugArgs {
    fn execute(self, _ctx: &super::GlobalContext) -> color_eyre::Result<()> {
        match self.action {
            DebugAction::InstallLldbServer { package, arch } => {
                install_lldb_server(&package, arch.as_deref())?;
            }
            DebugAction::StartLldbServer { package, port } => {
                start_lldb_server(&package, port)?;
            }
            DebugAction::Attach {
                package,
                arch,
                port,
                code_bin,
                open,
                symbols,
            } => {
                install_lldb_server(&package, arch.as_deref())?;
                start_lldb_server(&package, port)?;
                attach(&package, port, &code_bin, open, &symbols)?;
            }
        }

        Ok(())
    }
}

/// Queries the connected device's CPU ABI via `adb shell getprop ro.product.cpu.abi`
fn detect_device_arch() -> color_eyre::Result<String> {
    let output = std::process::Command::new(adb_path())
        .args(["shell", "getprop", "ro.product.cpu.abi"])
        .output()
        .context("Failed to query the device's CPU ABI via adb")?;
    let abi = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if abi.is_empty() {
        bail!("Could not detect the device's CPU ABI; pass the architecture explicitly");
    }
    Ok(abi)
}

fn adb_status(args: &[&str]) -> color_eyre::Result<()> {
    let status = std::process::Command::new(adb_path())
        .args(args)
        .status()
        .with_context(|| format!("Failed to run `adb {}`", args.join(" ")))?;
    if !status.success() {
        bail!("`adb {}` exited with status {status}", args.join(" "));
    }
    Ok(())
}

fn resolve_arch(arch: Option<&str>) -> color_eyre::Result<String> {
    match arch {
        Some(arch) => Ok(arch.to_string()),
        None => detect_device_arch(),
    }
}

const APP_LLDB_SERVER_PATH: &str = "./lldb-server";

/// Installs lldb-server into `package`'s private storage, in order:
/// 1. `adb push` lldb-server to shared storage (can't push directly into an app's
///    private storage).
/// 2. `chmod` it executable there.
/// 3. `run-as` `cp` it into `package`'s private storage, since lldb-server has to
///    run under the app's own SELinux/UID context to be allowed to attach to it.
/// 4. `chmod` it executable again (the copy doesn't preserve permissions).
fn install_lldb_server(package: &str, arch: Option<&str>) -> color_eyre::Result<()> {
    let arch = resolve_arch(arch)?;
    let lldb_server_path = constants::lldb_server_path(&arch);
    let device_tmp_path = "/data/local/tmp/lldb-server";

    println!("Pushing lldb-server ({arch}) to device...");
    adb_status(&["push", &lldb_server_path, device_tmp_path])
        .context("Failed to push lldb-server to device")?;
    adb_status(&["shell", "chmod", "755", device_tmp_path])
        .context("Failed to chmod lldb-server on device")?;

    println!("Copying lldb-server into {package}'s private storage...");
    adb_status(&[
        "shell",
        "run-as",
        package,
        "cp",
        device_tmp_path,
        APP_LLDB_SERVER_PATH,
    ])
    .with_context(|| {
        format!(
            "Failed to copy lldb-server into {package}'s storage \
             (is the app installed and debuggable?)"
        )
    })?;
    adb_status(&[
        "shell",
        "run-as",
        package,
        "chmod",
        "755",
        APP_LLDB_SERVER_PATH,
    ])
    .context("Failed to chmod lldb-server in app storage")?;

    println!("lldb-server installed in {package}'s private storage.");
    Ok(())
}

/// True if lldb-server is already sitting in `package`'s private storage.
fn lldb_server_installed(package: &str) -> color_eyre::Result<bool> {
    let status = std::process::Command::new(adb_path())
        .args(["shell", "run-as", package, "test", "-e", APP_LLDB_SERVER_PATH])
        .status()
        .context("Failed to check for lldb-server in app storage")?;
    Ok(status.success())
}

/// Forwards `port` and launches the lldb-server already installed (see
/// `install_lldb_server`) in `package`'s private storage as a platform server,
/// in the background, so it's still listening after this function returns.
fn start_lldb_server(package: &str, port: u16) -> color_eyre::Result<()> {
    if !lldb_server_installed(package)? {
        bail!(
            "lldb-server is not installed in {package}'s private storage; \
             run `install-lldb-server {package}` first"
        );
    }

    println!("Forwarding tcp:{port} to the device...");
    adb_status(&["forward", &format!("tcp:{port}"), &format!("tcp:{port}")])
        .context("Failed to forward debug port")?;

    println!("Starting lldb-server platform server on the device...");
    std::process::Command::new(adb_path())
        .args([
            "shell",
            "run-as",
            package,
            APP_LLDB_SERVER_PATH,
            "platform",
            "--listen",
            &format!("*:{port}"),
            "--server",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start lldb-server")?;

    // Give it a moment to start listening before callers try to connect.
    thread::sleep(Duration::from_millis(500));

    println!("lldb-server is listening on port {port}.");
    Ok(())
}

/// Resolves a symbols path into a list of files: passed through as-is if it's a
/// file, or recursively searched for `.so` files if it's a directory.
fn collect_so_files(path: &Path, out: &mut Vec<PathBuf>) -> color_eyre::Result<()> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path)
            .with_context(|| format!("Failed to read symbols directory {}", path.display()))?
        {
            let entry_path = entry?.path();
            if entry_path.is_dir() {
                collect_so_files(&entry_path, out)?;
            } else if entry_path.extension().is_some_and(|ext| ext == "so") {
                out.push(entry_path);
            }
        }
    } else {
        out.push(path.to_path_buf());
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct DapAttachConfig {
    request: &'static str,
    pid: u32,
    #[serde(rename = "attachCommands")]
    attach_commands: Vec<String>,
}


/// Force-stops and relaunches `package`, then opens an lldb-dap attach session
/// pointed at the lldb-server started by `start_lldb_server`.
/// Gets `package` into a debuggable, attached-to state, in order:
/// 1. `am force-stop` the app, so step 2 launches it fresh.
/// 2. `monkey` launch it back up via its default launcher intent.
/// 3. Poll `pidof` until the new process shows up.
/// 4. Resolve any `--symbols` directories to their `.so` files.
/// 5. Build the lldb `attachCommands` (platform select/connect to the lldb-server
///    started by `start_lldb_server`, `attach -p <pid>`, signal handling, symbol
///    loading) and wrap them in an lldb-dap attach config.
/// 6. Print the resulting `vscode://` attach URI, and open it with `code
///    --open-url` if requested.
fn attach(
    package: &str,
    port: u16,
    code_bin: &str,
    open_vscode: bool,
    symbols: &[PathBuf],
) -> color_eyre::Result<()> {
    println!("Restarting {package}...");
    adb_status(&["shell", "am", "force-stop", package]).context("Failed to force-stop the app")?;
    adb_status(&[
        "shell",
        "monkey",
        "-p",
        package,
        "-c",
        "android.intent.category.LAUNCHER",
        "1",
    ])
    .context("Failed to launch the app")?;

    println!("Waiting for {package} to start...");
    let pid = (0..20)
        .find_map(|_| {
            let output = std::process::Command::new(adb_path())
                .args(["shell", "pidof", package])
                .output()
                .ok()?;
            let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if pid.is_empty() {
                thread::sleep(Duration::from_millis(500));
                None
            } else {
                Some(pid)
            }
        })
        .context("Timed out waiting for the app to start")?;
    println!("{package} is running with PID {pid}");

    let mut symbol_files = Vec::new();
    for path in symbols {
        collect_so_files(path, &mut symbol_files)
            .with_context(|| format!("Failed to resolve symbols path {}", path.display()))?;
    }

    // lldb-dap's `attachCommands`, when present, take the place of its built-in
    // attach-by-pid logic entirely, so the platform select/connect and the actual
    // `attach` all have to happen inside it. Port forwarding maps this host's
    // tcp:port to the same port on the device, so lldb should connect to
    // localhost, not the device's adb serial.
    let mut attach_commands = vec![
        "platform select remote-android".to_string(),
        "settings set target.inherit-env false".to_string(),
        format!("platform connect connect://localhost:{port}"),
        format!("attach -p {pid}"),
        "pro hand -p true -s false SIGPWR".to_string(),
        "pro hand -p true -s false SIGXCPU".to_string(),
        "pro hand -p true -s false SIG33".to_string(),
    ];
    attach_commands.extend(
        symbol_files
            .iter()
            .map(|p| format!("target symbols add {}", p.display())),
    );

    let config = DapAttachConfig {
        request: "attach",
        pid: pid
            .parse()
            .with_context(|| format!("adb reported a non-numeric pid: {pid}"))?,
        attach_commands,
    };
    let config_json =
        serde_json::to_string(&config).context("Failed to serialize lldb-dap attach config")?;

    let debug_uri = format!(
        "vscode://llvm-vs-code-extensions.lldb-dap/start?config={}",
        percent_encoding::utf8_percent_encode(&config_json, percent_encoding::NON_ALPHANUMERIC)
    );

    println!("Port: {port}");
    println!("Attach URI: {debug_uri}");
    println!("PID: {}", config.pid);
    println!("Config: {config_json}");

    if open_vscode {
        println!("Opening VS Code debug session...");
        match std::process::Command::new(code_bin)
            .arg("--open-url")
            .arg(&debug_uri)
            .status()
        {
            Ok(status) if !status.success() => {
                eprintln!("`{code_bin} --open-url` exited with status {status}");
            }
            Err(err) => {
                eprintln!(
                    "Failed to invoke VS Code (`{code_bin}`): {err}. \
                     Is it on PATH? Pass --code-bin to override, or open the attach URI above manually."
                );
            }
            _ => {}
        }
    }

    Ok(())
}
