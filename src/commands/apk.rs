use std::{fs::OpenOptions, io::Cursor, path::PathBuf};

// use bytes::{BufMut, Bytes, BytesMut};
use color_eyre::eyre::{Context, ContextCompat, bail, eyre};
use mbf_res_man::version_grabber;
use mbf_zip::FileCompression;
use semver::Version;

use crate::{
    commands::Command,
    constants,
};
use mbf_axml::{AxmlReader, AxmlWriter, axml_to_xml, xml_to_axml};

#[derive(clap::Parser, Debug)]
pub struct ApkArgs {
    #[command(subcommand)]
    action: ApkAction,
}

#[derive(clap::Subcommand, Debug)]
pub enum ApkAction {
    Download {
        /// Oculus auth token, can be found in the browser devtools when logged in to oculus.com
        #[arg(long)]
        token: String,
        /// The ID of the APK to download, e.g. "com.beatgames.beatsaber" is 2448060205267927
        #[arg(long, default_value = "2448060205267927")]
        graph_app_id: String,
        /// The version of the APK to download, e.g. "1.0.0"
        version: String,

        /// Output path, defaults to current directory
        output: Option<PathBuf>,

        #[arg(long, default_value_t = true)]
        patch: bool,

        /// Installs APK after download
        #[arg(long, default_value_t = false)]
        install: bool,
    },
    Patch {
        path: PathBuf,
    },
}
const MANIFEST_FILE: &str = "AndroidManifest.xml";
const CERT_PEM: &[u8] = include_bytes!("../debug_cert.pem");

impl Command for ApkArgs {
    fn execute(self, _ctx: &crate::commands::GlobalContext) -> color_eyre::Result<()> {
        match self.action {
            ApkAction::Patch { path } => {
                do_patch(path)?;
            }
            ApkAction::Download {
                token,
                graph_app_id: apk_graph_id,
                version,
                output,
                patch,
                install,
            } => {
                let versions = version_grabber::get_live_versions(
                    &token,
                    Version::new(0, 0, 0),
                    &apk_graph_id,
                )
                .map_err(|e| eyre!(e))?;

                println!("Downloading {} version {}", apk_graph_id, version);

                let output = output.unwrap_or("./apk".into());
                let downloaded = version_grabber::download_version(
                    &token, &versions, &version, false, &output, false,
                )
                .map_err(|e| eyre!(e))?
                .context("Version not found")?;
                let version_folder = output.join(&downloaded.main.version);

                println!("Downloaded {} version {}", apk_graph_id, version);
                match patch {
                    true => {
                        println!("Patching APK");
                        do_patch(version_folder.join(format!("{}.apk", &downloaded.main.id)))?
                    },
                    false => {
                        println!(
                            "You may need to patch the APK to work in the emulator using `apk patch`",
                        );
                    }
                }

                if install {
                    println!("Installing APK");
                    let adb_path = constants::android_sdk_path()
                        .join("platform-tools")
                        .join("adb");

                    std::process::Command::new(&adb_path)
                        .arg("install")
                        .arg(&version_folder.join(format!("{}.apk", downloaded.main.id)))
                        .status()
                        .context("Failed to install APK")?;
                    
                    let obb_binary = version_folder.join(format!("main.{}.com.beatgames.beatsaber.obb", downloaded.main.version_code));
                    if obb_binary.exists() {
                        std::process::Command::new(&adb_path)
                            .arg("shell")
                            .arg("mkdir")
                            .arg("-p")
                            .arg("/sdcard/Android/obb/com.beatgames.beatsaber")
                            .status()
                            .context("Failed to create obb directory")?;

                        std::process::Command::new(&adb_path)
                            .arg("push")
                            .arg(&obb_binary)
                            .arg("/sdcard/Android/obb/com.beatgames.beatsaber")
                            .status()
                            .context("Failed to copy obb")?;
                    }

                    std::fs::remove_dir_all(&output).context("Failed to remove apk directory")?;
                    println!("Successfully installed APK");
                }
            }
        }
        Ok(())
    }
}

fn do_patch(path: PathBuf) -> Result<(), color_eyre::eyre::Error> {
    println!("Patching APK from path: {path:?}");
    let apk_file = OpenOptions::new()
        .write(true)
        .read(true)
        .open(&path)
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
