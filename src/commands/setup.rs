use std::{io::Cursor, path::PathBuf};

use bytes::{BufMut, BytesMut};
use color_eyre::eyre::{Context, bail};

use crate::{
    commands::{Command, GlobalContext},
    constants::{
        self, ANDROID_SDK_TOOLS, android_sdk_path, avd_path, cmdline_tools_path, emulator_path,
    },
    downloader,
};

#[derive(clap::Parser)]
pub struct SetupArgs {
    /// Install the Android SDK tools
    /// This includes the SDK Manager, platform tools, and build tools
    /// By default, this is prompted if the tools are not found
    #[arg(long = "sdk", default_value_t = false)]
    install_sdk: bool,

    /// Install the Android Emulator and system image
    #[arg(long = "emulator", default_value_t = false)]
    install_emulator: bool,

    /// Create an AVD (Android Virtual Device)
    #[arg(long = "avd", default_value_t = false)]
    create_avd: bool,

    /// Overwrite existing AVD (Android Virtual Device)
    #[arg(long = "overwrite", default_value_t = false)]
    overwrite_avd: bool,

    /// Name of AVD (Android Virtual Device)
    #[arg(long, default_value_t = constants::DEFAULT_AVD_NAME.to_string())]
    name: String,

    /// Screen size for the AVD (Android Virtual Device), e.g. "1920x1080"
    #[arg(long = "screen-size", default_value = "1920x1080")]
    screen_size: String,

    /// System image of AVD (Android Virtual Device)
    #[arg(long = "image", default_value_t = constants::DEFAULT_AVD_IMAGE.to_string())]
    system_image: String,

    /// Path to the Android SDK Manager
    #[arg(long)]
    sdk_manager_path: Option<PathBuf>,
}

impl Command for SetupArgs {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()> {
        let sdk_manager = constants::sdkmanager_path();

        println!("Using Android SDK path: {}", android_sdk_path().display());

        if !sdk_manager.exists() {
            let accepted = ctx.yes
                || self.install_sdk
                || dialoguer::Confirm::new()
                    .with_prompt(
                        "Android SDK Manager not found. Do you want to download and set it up?",
                    )
                    .interact()?;
            if accepted {
                setup_sdk_manager().context("Failed to set up SDK Manager")?;
            }
        }

        let android_emu_image_installed = constants::emulator_path().exists()
            && constants::android_sdk_path()
                .join("platform-tools")
                .join("adb")
                .exists()
            && constants::android_sdk_path()
                .join(self.system_image.replace(";", "/"))
                .exists();

        let android_emu_image = !android_emu_image_installed
            && (ctx.yes
                || self.install_emulator
                || dialoguer::Confirm::new()
                    .with_prompt("Do you want to install the Android Emulator and system image?")
                    .interact()?);
        if android_emu_image {
            install_tools(&sdk_manager, &self.system_image)?;
        }

        let mut avd_folder_name = self.name.clone();
        avd_folder_name.push_str(".avd");
        if avd_path().join(&avd_folder_name).exists() {
            let overwrite_avd = (ctx.yes || self.overwrite_avd)
                || dialoguer::Confirm::new()
                    .with_prompt("An existing AVD (Android Virtual Device) was found, do you want to delete this?")
                    .interact()?;

            match overwrite_avd {
                true => delete_emulator(&self.name)?,
                false => bail!("An existing AVD (Android Virtual Device) was found!"),
            }
        }

        let create_avd = ctx.yes
            || self.create_avd
            || dialoguer::Confirm::new()
                .with_prompt("Do you want to create an AVD (Android Virtual Device)?")
                .interact()?;
        if create_avd {
            create_emulator_with_screen_size(&self.name, &self.system_image, &self.screen_size)?;
            println!("Run the emulator using the 'emulator @{}'", self.name);
        }

        println!(
            "Add {} to your PATH.",
            sdk_manager.parent().unwrap().display()
        );
        println!(
            "Add {} to your PATH.",
            emulator_path().parent().unwrap().display()
        );

        Ok(())
    }
}

pub fn create_emulator(name: &str, image: &str) -> Result<(), color_eyre::eyre::Error> {
    let status = std::process::Command::new(constants::avdmanager_path())
        .arg("create")
        .arg("avd")
        .arg("-n")
        .arg(name)
        .arg("-k")
        .arg(image)
        .status()
        .context("Failed to create AVD (Android Virtual Device)")?;
    if !status.success() {
        bail!("avdmanager exited with status: {}", status);
    };
    Ok(())
}

/// Create an emulator with a specific screen size (e.g. "1080x1920")
pub fn create_emulator_with_screen_size(
    name: &str,
    image: &str,
    screen_size: &str,
) -> Result<(), color_eyre::eyre::Error> {
    // Create the AVD first
    create_emulator(name, image)?;
    // Set the screen size in config.ini
    let avd_dir = avd_path().join(format!("{}.avd", name));
    let config_path = avd_dir.join("config.ini");
    if config_path.exists() {
        use std::fs;
        use std::io::Write;
        let mut config = fs::OpenOptions::new().append(true).open(&config_path)?;
        writeln!(
            config,
            "hw.lcd.width={}",
            screen_size.split('x').next().unwrap_or("1080")
        )?;
        writeln!(
            config,
            "hw.lcd.height={}",
            screen_size.split('x').nth(1).unwrap_or("1920")
        )?;
    }
    Ok(())
}

pub fn delete_emulator(name: &str) -> Result<(), color_eyre::eyre::Error> {
    let status = std::process::Command::new(constants::avdmanager_path())
        .arg("delete")
        .arg("avd")
        .arg("-n")
        .arg(name)
        .status()
        .context("Failed to delete AVD (Android Virtual Device)")?;

    if !status.success() {
        bail!("avdmanager exited with status: {}", status);
    }
    Ok(())
}

pub fn install_tools(sdk_manager: &PathBuf, image: &str) -> Result<(), color_eyre::eyre::Error> {
    let status = std::process::Command::new(sdk_manager)
        .arg("emulator")
        .arg("platform-tools")
        .arg(image)
        .status()
        .context("Failed to run sdkmanager")?;
    if !status.success() {
        bail!("sdkmanager exited with status: {}", status);
    };
    Ok(())
}

pub fn setup_sdk_manager() -> color_eyre::Result<()> {
    println!("Android SDK Tools not found, downloading...");
    println!("Adding to path: {}", android_sdk_path().display());

    let mut zip_tmp = BytesMut::new().writer();
    downloader::download_with_progress(None, ANDROID_SDK_TOOLS, &mut zip_tmp)
        .context("Failed to download Android SDK Tools")?;

    let zip_cursor = Cursor::new(zip_tmp.into_inner());

    let mut zip = zip::ZipArchive::new(zip_cursor).context("Failed to read downloaded zip file")?;
    zip.extract_unwrapped_root_dir(cmdline_tools_path(), zip::read::root_dir_common_filter)
        .context("Failed to extract Android SDK Tools")?;
    Ok(())
}
