use std::{
    fs::OpenOptions,
    io::Cursor,
    path::{Path, PathBuf},
};

// use bytes::{BufMut, Bytes, BytesMut};
use color_eyre::eyre::{Context, ContextCompat, bail, eyre};
use mbf_res_man::version_grabber;
use mbf_zip::FileCompression;
use semver::Version;

use crate::{
    commands::Command,
    constants::{self, adb_path},
};
use mbf_axml::{AxmlReader, AxmlWriter, axml_to_xml, xml_to_axml};

#[derive(clap::Parser, Debug)]
pub struct ApkArgs {
    #[command(subcommand)]
    action: ApkAction,
}

#[derive(clap::Subcommand, Debug)]
pub enum ApkAction {
    /// Download an APK from Oculus Graph and optionally patch and install it
    Download {
        /// Oculus auth token, can be found in the browser devtools when logged in to oculus.com
        #[arg(long)]
        token: String,
        /// The ID of the APK to download, e.g. "com.beatgames.beatsaber" is 2448060205267927
        #[arg(long, default_value = "2448060205267927")]
        graph_app_id: String,
        /// The version of the APK to download, e.g. "1.0.0"
        fuzzy_version: String,

        /// Output path, defaults to current directory
        output: Option<PathBuf>,

        /// Patches the APK after download to work in the emulator
        #[arg(long, default_value_t = true)]
        patch: bool,

        /// Installs APK and obb after download
        #[arg(long, default_value_t = false)]
        install: bool,
    },
    /// Patch an APK to work in the emulator.
    Patch {
        /// Path to the APK to patch
        path: PathBuf,
    },
    /// Install an APK and its OBB file to the emulator
    Install {
        /// The ID of the APK to install. E.g. "com.beatgames.beatsaber"
        apk_id: String,
        /// The folder where the APK and OBB files are located. Directory looks like:
        /// <output_folder>/
        ///     <apk_id>.apk
        ///     main.<version_code>.<apk_id>.obb (optional)
        folder_path: PathBuf,
    },
}
const MANIFEST_FILE: &str = "AndroidManifest.xml";
const CERT_PEM: &[u8] = include_bytes!("../debug_cert.pem");

impl Command for ApkArgs {
    fn execute(self, _ctx: &crate::commands::GlobalContext) -> color_eyre::Result<()> {
        match self.action {
            ApkAction::Patch { path } => {
                do_patch(&path)?;
            }
            ApkAction::Install {
                apk_id,
                folder_path,
            } => {
                let apk_path = folder_path.join(format!("{}.apk", &apk_id));
                if !apk_path.exists() {
                    bail!("APK file not found at path: {:?}", apk_path);
                }

                // Find the obb file if it exists
                let obb_files: Vec<_> = std::fs::read_dir(&folder_path)?
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let file_name = entry.file_name().to_string_lossy().to_string();
                        if file_name.starts_with("main.")
                            && file_name.ends_with(&format!(".{}", &apk_id))
                        {
                            Some(entry.path())
                        } else {
                            None
                        }
                    })
                    .collect();

                let obb_path = obb_files
                    .first()
                    .cloned()
                    .unwrap_or_else(|| folder_path.join(""));

                do_install(&folder_path, &apk_id, &apk_path, &obb_path)?;
            }
            ApkAction::Download {
                token,
                graph_app_id,
                fuzzy_version,
                output,
                patch,
                install,
            } => {
                let versions = version_grabber::get_live_versions(
                    &token,
                    Version::new(0, 0, 0),
                    &graph_app_id,
                )
                .map_err(|e| eyre!(e))?;

                let matching_version: &str = versions
                    // 1) Try exact match
                    .iter()
                    .find_map(|(v, _)| {
                        if v.non_semver == fuzzy_version {
                            Some(&v.non_semver)
                        } else {
                            None
                        }
                    })
                    // 2) If no exact match, try a fuzzy match (contains / contained-by)
                    .or_else(|| {
                        versions.iter().find_map(|(v, _)| {
                            let ns = &v.non_semver;
                            if ns.contains(&fuzzy_version) || fuzzy_version.contains(ns) {
                                Some(ns)
                            } else {
                                None
                            }
                        })
                    })
                    // convert Option<&String> -> Option<&str> and fallback to the original input
                    .map(|s| s.as_str())
                    .unwrap_or(&fuzzy_version);

                println!("Downloading {} version {}", graph_app_id, matching_version);

                let output = output.unwrap_or("./apk".into());
                let downloaded = version_grabber::download_version(
                    &token,
                    &versions,
                    matching_version,
                    false,
                    &output,
                    false,
                )
                .map_err(|e| eyre!(e))?
                .context("Version not found")?;

                let version_folder = output.join(&downloaded.main.version);
                let apk_path = version_folder.join(format!("{}.apk", &downloaded.main.id));
                let obb_path = version_folder.join(format!(
                    "main.{}.{}.obb",
                    downloaded.main.version_code, downloaded.main.id,
                ));

                println!("Downloaded {} version {}", downloaded.main.id, fuzzy_version);
                match patch {
                    true => {
                        println!("Patching APK");
                        do_patch(&apk_path)?
                    }
                    false => {
                        println!(
                            "You may need to patch the APK to work in the emulator using `apk patch`",
                        );
                    }
                }

                if install {
                    do_install(&output, &downloaded.main.id, &apk_path, &obb_path)?;
                }
            }
        }
        Ok(())
    }
}

fn do_install(
    output: &Path,
    apk_id: &str,
    apk_path: &Path,
    obb_binary: &Path,
) -> Result<(), color_eyre::eyre::Error> {
    println!("Installing APK");
    let adb_path = adb_path();

    std::process::Command::new(&adb_path)
        .arg("install")
        .arg(apk_path)
        .status()
        .context("Failed to install APK")?;
    if obb_binary.exists() {
        let obb_device_path = format!("/sdcard/Android/obb/{}", apk_id);
        std::process::Command::new(&adb_path)
            .arg("shell")
            .arg("mkdir")
            .arg("-p")
            .arg(&obb_device_path)
            .status()
            .context("Failed to create obb directory")?;

        std::process::Command::new(&adb_path)
            .arg("push")
            .arg(obb_binary)
            .arg(obb_device_path)
            .status()
            .context("Failed to copy obb")?;
    }
    std::fs::remove_dir_all(output).context("Failed to remove apk directory")?;
    println!("Successfully installed APK");
    Ok(())
}

fn do_patch(path: &Path) -> Result<(), color_eyre::eyre::Error> {
    println!("Patching APK from path: {path:?}");
    let apk_file = OpenOptions::new()
        .write(true)
        .read(true)
        .open(path)
        .context("")?;
    let mut apk = mbf_zip::ZipFile::open(apk_file)
        .map_err(|a| color_eyre::eyre::eyre!(a))
        .context("Failed to read APK as zip file")?;
    let manifest_bytes = apk
        .read_file(MANIFEST_FILE)
        .map_err(|a| color_eyre::eyre::eyre!(a))
        .context("Failed to read AndroidManifest.xml from APK")?;
    let axml_bytes = patch_manifest(manifest_bytes)?;
    let mut axml_cursor = Cursor::new(axml_bytes);
    apk.write_file(MANIFEST_FILE, &mut axml_cursor, FileCompression::Store)
        .map_err(|a| color_eyre::eyre::eyre!(a))
        .context("Failed to write modified AndroidManifest.xml back to APK")?;
    let (cert, priv_key) = mbf_zip::signing::load_cert_and_priv_key(CERT_PEM);
    apk.save_and_sign_v2(&priv_key, &cert)
        .map_err(|a| color_eyre::eyre::eyre!(a))
        .context("Failed to save modified APK")?;
    println!("Successfully patched AndroidManifest.xml");
    Ok(())
}

/// AI generated code to patch the manifest
/// I'm too lazy to do it myself
/// since it's XML :(
fn patch_manifest(manifest_bytes: Vec<u8>) -> Result<Vec<u8>, color_eyre::eyre::Error> {
    let mut manifest_cursor = Cursor::new(manifest_bytes);
    let mut axml_reader = AxmlReader::new(&mut manifest_cursor)
        .map_err(|a| color_eyre::eyre::eyre!(a))
        .context("Failed to parse AndroidManifest.xml as AXML")?;
    let mut xml_bytes = Vec::new();
    {
        let mut writer = xml::EventWriter::new(&mut xml_bytes);
        axml_to_xml(&mut writer, &mut axml_reader).map_err(|a| color_eyre::eyre::eyre!(a))?;
    }
    let mut xml_str = String::from_utf8(xml_bytes)?;
    let insert_str = r#"  <queries>    <package android:name="com.oculus.horizon"/>  </queries>"#;

    // Check if the <queries> block with the package is already present
    if !xml_str.contains(r#"<package android:name="com.oculus.horizon""#) {
        match xml_str.rfind("</manifest>") {
            Some(idx) => {
                xml_str.insert_str(idx, insert_str);
            }
            None => {
                bail!("No </manifest> tag found in manifest");
            }
        }
    }
    let mut axml_bytes = Vec::new();
    {
        let mut axml_writer = AxmlWriter::new(&mut axml_bytes);
        let mut xml_reader = xml::EventReader::from_str(&xml_str);
        xml_to_axml(&mut axml_writer, &mut xml_reader).map_err(|a| color_eyre::eyre::eyre!(a))?;
        axml_writer
            .finish()
            .map_err(|a| color_eyre::eyre::eyre!(a))?;
    }
    Ok(axml_bytes)
}
