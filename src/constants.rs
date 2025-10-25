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

/// Returns the path to the Android SDK
/// Checks the ANDROID_SDK_ROOT and ANDROID_HOME environment variables
/// If not set, defaults to {home}/Android/Sdk
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

/// Returns the path to the Android cmdline-tools latest directory
/// {sdk}/cmdline-tools/latest
pub fn cmdline_tools_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("cmdline-tools");
    path.push("latest");
    path
}

/// Returns the path to the Android SDK Manager executable
/// {sdk}/cmdline-tools/latest/bin/sdkmanager[.bat]
pub fn sdkmanager_path() -> PathBuf {
    let mut path = cmdline_tools_path();
    path.push("bin");
    path.push("sdkmanager");
    if cfg!(target_os = "windows") {
        path.set_extension("bat");
    }
    path
}

/// Returns the path to the Android AVD Manager executable
/// {sdk}/cmdline-tools/latest/bin/avdmanager[.bat]
pub fn avdmanager_path() -> PathBuf {
    let mut path = cmdline_tools_path();
    path.push("bin");
    path.push("avdmanager");
    if cfg!(target_os = "windows") {
        path.set_extension("bat");
    }
    path
}

/// Returns the path to the Android AVDs
/// {home}/.android/avd or $ANDROID_AVD_HOME
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

/// Returns the path to the Android Emulator executable
/// {sdk}/emulator/emulator[.exe]
pub fn emulator_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("emulator");
    path.push("emulator");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}

/// Returns the path to the adb executable
/// {sdk}/platform-tools/adb[.exe]
pub fn adb_path() -> PathBuf {
    let mut path = android_sdk_path();
    path.push("platform-tools");
    path.push("adb");
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }
    path
}
