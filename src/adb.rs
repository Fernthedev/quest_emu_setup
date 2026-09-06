use std::{thread, time::Duration};

use color_eyre::eyre::{Context, ContextCompat, bail};

use crate::constants::adb_path;

/// Polls `adb shell pidof` until `package` shows up as a running process.
pub fn wait_for_pid(package: &str) -> color_eyre::Result<String> {
    println!("Waiting for {package} to start...");
    (0..20)
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
        .context("Timed out waiting for the app to start")
}


pub fn adb_status(args: &[&str]) -> color_eyre::Result<()> {
    let status = std::process::Command::new(adb_path())
        .args(args)
        .status()
        .with_context(|| format!("Failed to run `adb {}`", args.join(" ")))?;
    if !status.success() {
        bail!("`adb {}` exited with status {status}", args.join(" "));
    }
    Ok(())
}


/// Resolves `package`'s launcher activity component (`package/.Activity`) via
/// `cmd package resolve-activity`, for use with `am start -n`. Needed because
/// `am start -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -p
/// <package>` fails to resolve for activities that declare additional categories
/// alongside LAUNCHER (e.g. Quest apps also declaring `com.oculus.intent.category.VR`,
/// which makes them non-`isDefault` and unresolvable by that implicit-intent form)
/// even though the exact same activity resolves fine here and via `monkey`.
pub fn resolve_launch_component(package: &str) -> color_eyre::Result<String> {
    let output = std::process::Command::new(adb_path())
        .args([
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "-c",
            "android.intent.category.LAUNCHER",
            package,
        ])
        .output()
        .context("Failed to resolve the app's launcher activity via adb")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| line.contains('/'))
        .map(str::to_string)
        .with_context(|| {
            format!(
                "Could not resolve a launcher activity for {package}: {}",
                stdout.trim()
            )
        })
}

/// Queries the connected device's CPU ABI via `adb shell getprop ro.product.cpu.abi`
pub fn detect_device_arch() -> color_eyre::Result<String> {
    let output = std::process::Command::new(adb_path())
        .args(["shell", "getprop", "ro.product.cpu.abi"])
        .output()
        .context("Failed to query the device's CPU ABI via adb")?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        bail!(
            "`adb shell getprop ro.product.cpu.abi` exited with status {}{}",
            output.status,
            if stderr.is_empty() { String::new() } else { format!(": {stderr}") }
        );
    }

    let abi = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if abi.is_empty() {
        bail!(
            "Could not detect the device's CPU ABI (empty response from adb); \
             pass the architecture explicitly{}",
            if stderr.is_empty() { String::new() } else { format!(". adb stderr: {stderr}") }
        );
    }
    Ok(abi)
}
