use std::{
    fs::OpenOptions,
    io::{Cursor, Write},
    path::PathBuf,
};

// use bytes::{BufMut, Bytes, BytesMut};
use color_eyre::eyre::{Context, bail};
use mbf_zip::FileCompression;

use crate::commands::Command;
use mbf_axml::{AxmlReader, AxmlWriter, axml_to_xml, xml_to_axml};

#[derive(clap::Parser, Debug)]
pub struct ApkArgs {
    #[command(subcommand)]
    action: ApkAction,
}

#[derive(clap::Subcommand, Debug)]
pub enum ApkAction {
    Patch { path: PathBuf },
}
const MANIFEST_FILE: &str = "AndroidManifest.xml";

impl Command for ApkArgs {
    fn execute(self, _ctx: &crate::commands::GlobalContext) -> color_eyre::Result<()> {
        match self.action {
            ApkAction::Patch { path } => {
                println!("Patching APK from path: {path:?}");
                let apk_file = OpenOptions::new()
                    .write(true)
                    .read(true)
                    .open(&path)
                    .context("")?;
                let mut apk = mbf_zip::ZipFile::open(apk_file)
                    .map_err(|a| color_eyre::eyre::eyre!(a))
                    .context("Failed to read APK as zip file")?;

                // Step 1: Extract and decode manifest
                let manifest_bytes = apk
                    .read_file(MANIFEST_FILE)
                    .map_err(|a| color_eyre::eyre::eyre!(a))
                    .context("Failed to read AndroidManifest.xml from APK")?;
                let axml_bytes = patch_manifest(manifest_bytes)?;
                // Step 4: Write back to APK
                let mut axml_cursor = Cursor::new(axml_bytes);
                apk.write_file(MANIFEST_FILE, &mut axml_cursor, FileCompression::Store)
                    .map_err(|a| color_eyre::eyre::eyre!(a))
                    .context("Failed to write modified AndroidManifest.xml back to APK")?;
                const CERT_PEM: &[u8] = include_bytes!("../debug_cert.pem");
                let (cert, priv_key) = mbf_zip::signing::load_cert_and_priv_key(CERT_PEM);
                apk.save_and_sign_v2(&priv_key, &cert)
                    .map_err(|a| color_eyre::eyre::eyre!(a))
                    .context("Failed to save modified APK")?;
                println!("Successfully patched AndroidManifest.xml");
            }
        }
        Ok(())
    }
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
