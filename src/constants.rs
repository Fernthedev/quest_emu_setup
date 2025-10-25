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

pub const DEFAULT_AVD_NAME: &str = "android13desktop";

pub const DEFAULT_AVD_IMAGE: &str = "system-images;android-33;android-desktop;x86_64";

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

pub fn cmdline_tools_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("cmdline-tools");
    path.push("latest");
    path
}

pub fn sdkmanager_path() -> PathBuf {
    let mut path = cmdline_tools_path();
    path.push("bin");
    path.push("sdkmanager");
    if cfg!(target_os = "windows") {
        path.set_extension("bat");
    }
    path
}

pub fn avdmanager_path() -> PathBuf {
    let mut path = cmdline_tools_path();
    path.push("bin");
    path.push("avdmanager");
    if cfg!(target_os = "windows") {
        path.set_extension("bat");
    }
    path
}

pub fn avd_path() -> PathBuf {
    std::env::var("ANDROID_AVD_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let home = dirs::home_dir()?;
            Some(home.join(".android").join("avd"))
        })
        .expect(
            "Could not find Android AVD path. Please set ANDROID_AVD_HOME environment variable.",
        )
}

pub fn emulator_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("emulator");
    path.push("emulator");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}

pub fn adb_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("platform-tools");
    path.push("adb");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}
