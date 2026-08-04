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

use color_eyre::eyre::Context;

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
    /// target app's private storage. Does not start it; use `attach` for that.
    /// The app must be debuggable (`android:debuggable="true"`).
    Install {
        /// Android package name of the app to debug, e.g. "com.beatgames.beatsaber"
        package: String,

        /// The architecture of the target device (e.g. "aarch64", "x86_64").
        /// Auto-detected from the device via `adb` if omitted.
        arch: Option<String>,
    },

    /// Install lldb-server (see `install`), launch it as a debug platform server,
    /// then attach to the app as it's already running (pass `--relaunch` to
    /// force-stop and relaunch it fresh first) and open a VS Code attach session
    /// for it via the lldb-dap extension (llvm-vs-code-extensions.lldb-dap).
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

        /// Write the target's pid to this file (e.g. for a VS Code task to feed
        /// into a launch.json config via the Command Variable extension)
        #[cfg_attr(feature = "clap", arg(long))]
        pid_file: Option<PathBuf>,

        /// Force-stop and relaunch the app fresh before attaching, instead of
        /// attaching to it as it's already running
        #[cfg_attr(feature = "clap", arg(long, default_value_t = false))]
        relaunch: bool,
    },
}

impl Command for DebugArgs {
    fn execute(self, _ctx: &super::GlobalContext) -> color_eyre::Result<()> {
        match self.action {
            DebugAction::Install { package, arch } => {
                install_lldb_server(&package, arch.as_deref())?;
            }
            DebugAction::Attach {
                package,
                arch,
                port,
                code_bin,
                open,
                symbols,
                pid_file,
                relaunch,
            } => {
                install_lldb_server(&package, arch.as_deref())?;
                start_lldb_platform_server(&package, port)?;
                attach(
                    &package,
                    port,
                    &code_bin,
                    open,
                    &symbols,
                    pid_file.as_deref(),
                    relaunch,
                )?;
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
/// in the background, so it's still listening after this function returns.
fn start_lldb_platform_server(package: &str, port: u16) -> color_eyre::Result<()> {
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

    Ok(())
}

/// Attaches to `package` as it's already running, or (if `relaunch`) force-stops
/// and relaunches it fresh first, then opens an lldb-dap attach session pointed
/// at the lldb-server started by `start_lldb_platform_server`, in order:
/// 1. If `relaunch`: `am force-stop` the app, then `monkey` launch it back up
///    via its default launcher intent, so it comes back with a fresh pid.
/// 2. Wait for the process to show up and build its attach info (see
///    `wait_for_pid`/`build_attach_info`).
/// 3. Print it, and open it with `code --open-url` if requested.
fn attach(
    package: &str,
    port: u16,
    code_bin: &str,
    open_vscode: bool,
    symbols: &[PathBuf],
    pid_file: Option<&Path>,
    relaunch: bool,
) -> color_eyre::Result<()> {
    if relaunch {
        println!("Restarting {package}...");
        adb_status(&["shell", "am", "force-stop", package])
            .context("Failed to force-stop the app")?;
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
    } else {
        println!("Attaching to already-running {package}...");
    }

    let pid = adb::wait_for_pid(package)?;
    let info = lldb::build_attach_info(&pid, port, symbols)?;
    print_attach_info(port, &info);
    write_pid_file(pid_file, info.pid)?;

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

/// Writes the bare pid to `path`, if given, for a VS Code task to hand off to a
/// launch.json config (e.g. via the Command Variable extension) after this
/// process's `preLaunchTask` run finishes.
fn write_pid_file(path: Option<&Path>, pid: u32) -> color_eyre::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    std::fs::write(path, pid.to_string())
        .with_context(|| format!("Failed to write pid file at {}", path.display()))
}
