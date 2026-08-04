// # macOS / Linux path pattern
// <path-to-ndk>/toolchains/llvm/prebuilt/<host-os>/lib/clang/<version>/lib/linux/aarch64/lldb-server

// # Windows path pattern
// <path-to-ndk>\toolchains\llvm\prebuilt\windows-x86_64\lib\clang\<version>\lib\linux\aarch64\lldb-server

use std::{
    path::PathBuf,
    process::Stdio,
    thread,
    time::Duration,
};

use color_eyre::eyre::{Context, bail};

use crate::{
    adb::{self, adb_status, detect_device_arch}, commands::Command, constants::{self, adb_path}, lldb::{self, AttachInfo},
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
    Install {
        /// Android package name of the app to debug, e.g. "com.beatgames.beatsaber"
        package: String,

        /// The architecture of the target device (e.g. "aarch64", "x86_64").
        /// Auto-detected from the device via `adb` if omitted.
        arch: Option<String>,
    },

    /// Launch the lldb-server already installed (see `install-lldb-server`) in the
    /// target app's private storage as a debug platform server via `run-as`, and
    /// forward the debug port to the host.
    Start {
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
            DebugAction::Install { package, arch } => {
                install_lldb_server(&package, arch.as_deref())?;
            }
            DebugAction::Start { package, port } => {
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

fn resolve_arch(arch: Option<&str>) -> color_eyre::Result<String> {
    match arch {
        Some(arch) => Ok(arch.to_string()),
        None => detect_device_arch(),
    }
}

const APP_LLDB_SERVER_PATH: &str = "./lldb-server";

/// True if lldb-server is already sitting in `package`'s private storage.
fn lldb_server_installed(package: &str) -> color_eyre::Result<bool> {
    let status = std::process::Command::new(adb_path())
        .args(["shell", "run-as", package, "test", "-e", APP_LLDB_SERVER_PATH])
        .status()
        .context("Failed to check for lldb-server in app storage")?;
    Ok(status.success())
}

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
    adb_status(&["push", &lldb_server_path.display().to_string(), device_tmp_path])
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


/// Forwards `port` and launches the lldb-server already installed (see
/// `install_lldb_server`) in `package`'s private storage as a platform server,
/// in the background, so it's still listening after this function returns. Then
/// waits for `package` to be running and prints the lldb-dap attach info for it,
/// same as `attach` does, minus the force-stop/relaunch and opening VS Code.
fn start_lldb_server(package: &str, port: u16) -> color_eyre::Result<()> {
    if !lldb_server_installed(package)? {
        bail!(
            "lldb-server is not installed in {package}'s private storage; \
             run `install-lldb-server {package}` first"
        );
    }

    println!("Forwarding port {port}");
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

    let pid = adb::wait_for_pid(package)?;
    let info = lldb::build_attach_info(&pid, port, &[])?;
    print_attach_info(port, &info);
    
    Ok(())
}

/// Force-stops and relaunches `package`, then opens an lldb-dap attach session
/// pointed at the lldb-server started by `start_lldb_server`, in order:
/// 1. `am force-stop` the app, so step 2 launches it fresh.
/// 2. `monkey` launch it back up via its default launcher intent.
/// 3. Wait for the new process to show up and build its attach info (see
///    `wait_for_pid`/`build_attach_info`).
/// 4. Print it, and open it with `code --open-url` if requested.
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

    let pid = adb::wait_for_pid(package)?;
    let info = lldb::build_attach_info(&pid, port, symbols)?;
    print_attach_info(port, &info);

    if open_vscode {
        println!("Opening VS Code debug session...");
        match std::process::Command::new(code_bin)
            .arg("--open-url")
            .arg(&info.debug_uri)
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

/// Prints the 4 lines needed to actually connect a debugger: the forwarded
/// port, the lldb-dap attach URI, the target pid, and the raw config behind
/// that URI (for pasting into a manual DAP launch instead).
fn print_attach_info(port: u16, info: &AttachInfo) {
    println!("Port: {port}");
    println!("Attach URI: {}", info.debug_uri);
    println!("PID: {}", info.pid);
    println!("Config: {}", info.config_json);
}
