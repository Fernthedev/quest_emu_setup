use std::path::PathBuf;
use std::{env, io::Cursor};

use bytes::{BufMut, Bytes, BytesMut};
use color_eyre::eyre::{Context, bail};

use crate::constants::{ANDROID_SDK_TOOLS, android_sdk_path, emulator_path};

mod constants;
mod downloader;

fn main() -> color_eyre::Result<()> {
    let sdk_manager = constants::sdkmanager_path();

    if !sdk_manager.exists() {
        let accepted = dialoguer::Confirm::new()
            .with_prompt("Android SDK Manager not found. Do you want to download and set it up?")
            .interact()?;
        if accepted {
            setup_sdk_manager().context("Failed to set up SDK Manager")?;
        }
    }

    let android_emu_image = dialoguer::Confirm::new()
        .with_prompt("Do you want to install the Android Emulator and system image?")
        .interact()?;
    if android_emu_image {
        let status = std::process::Command::new(&sdk_manager)
            .arg("emulator")
            .arg("platform-tools;system-images;android-33;android-desktop;x86_64")
            .status()
            .context("Failed to run sdkmanager")?;

        if !status.success() {
            bail!("sdkmanager exited with status: {}", status);
        }
    }

    // TODO: Check if emulator image is already installed

    let create_avd = dialoguer::Confirm::new()
        .with_prompt("Do you want to create an AVD (Android Virtual Device)?")
        .interact()?;
    if create_avd {
        let status = std::process::Command::new(constants::avdmanager_path())
            .arg("create")
            .arg("avd")
            .arg("-n")
            .arg("android13desktop")
            .arg("-k")
            .arg("system-images;android-33;android-desktop;x86_64")
            .status()
            .context("Failed to run avdmanager")?;

        if !status.success() {
            bail!("avdmanager exited with status: {}", status);
        }
    }

    println!("Setup complete! You can now run the emulator using the 'emulator' command.");
    println!(
        "Add {} to your PATH.",
        sdk_manager.parent().unwrap().display()
    );
    println!(
        "Add {}/emulator to your PATH.",
        emulator_path().parent().unwrap().display()
    );

    Ok(())
}

fn setup_sdk_manager() -> color_eyre::Result<()> {
    let client = reqwest::blocking::Client::new();

    println!("Android SDK Tools not found, downloading...");
    let mut zip_tmp = BytesMut::new().writer();
    downloader::download_with_progress(&client, ANDROID_SDK_TOOLS, &mut zip_tmp)
        .context("Failed to download Android SDK Tools")?;

    let zip_cursor = Cursor::new(zip_tmp.into_inner());

    let mut zip = zip::ZipArchive::new(zip_cursor).context("Failed to read downloaded zip file")?;

    zip.extract(android_sdk_path())
        .context("Failed to extract Android SDK Tools")?;
    Ok(())
}
