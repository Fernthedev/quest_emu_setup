use std::{env, path::PathBuf};

#[cfg(target_os = "linux")]
pub const ANDROID_SDK_TOOLS: &str =
    "https://dl.google.com/android/repository/commandlinetools-linux-13114758_latest.zip";

#[cfg(target_os = "macos")]
pub const ANDROID_SDK_TOOLS: &str =
    "https://dl.google.com/android/repository/commandlinetools-mac-13114758_latest.zip";

#[cfg(target_os = "windows")]
pub const ANDROID_SDK_TOOLS: &str =
    "https://dl.google.com/android/repository/commandlinetools-win-13114758_latest.zip";

pub fn android_sdk_path() -> PathBuf {
    std::env::var("ANDROID_SDK_ROOT")
        .or_else(|_| env::var("ANDROID_HOME"))
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let home = dirs::home_dir()?;

            Some(home.join("Android/Sdk/"))
        })
        .expect("Could not find Android SDK path. Please set ANDROID_SDK_ROOT or ANDROID_HOME environment variable.")
}

pub fn sdkmanager_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("cmdline-tools/latest/bin/sdkmanager");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}

pub fn avdmanager_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("cmdline-tools/latest/bin/avdmanager");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}

pub fn emulator_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("emulator/emulator");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}