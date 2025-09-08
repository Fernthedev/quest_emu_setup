use std::{fs::File, io::Cursor, path::PathBuf};

// use bytes::{BufMut, Bytes, BytesMut};
use color_eyre::eyre::Context;
use mbf_zip::FileCompression;

use crate::axml::{AxmlReader, AxmlWriter, axml_to_xml, xml_to_axml};
use crate::commands::Command;

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
                let apk_file = File::open(&path).context("")?;
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
        .context("Failed to parse AndroidManifest.xml as AXML")?;
    let mut xml_bytes = Vec::new();
    {
        let mut writer = xml::EventWriter::new(&mut xml_bytes);
        axml_to_xml(&mut writer, &mut axml_reader)?;
    }
    let mut xml_str = String::from_utf8(xml_bytes)?;
    let insert_str =
        r#"  <queries>\n    <package android:name=\"com.oculus.horizon\"/>\n  </queries>\n"#;
    if let Some(idx) = xml_str.rfind("</manifest>") {
        xml_str.insert_str(idx, insert_str);
    } else {
        return Err(color_eyre::eyre::eyre!(
            "No </manifest> tag found in manifest"
        ));
    }
    let mut axml_bytes = Vec::new();
    {
        let mut axml_writer = AxmlWriter::new(&mut axml_bytes);
        let mut xml_reader = xml::EventReader::from_str(&xml_str);
        xml_to_axml(&mut axml_writer, &mut xml_reader)?;
        axml_writer.finish()?;
    }
    Ok(axml_bytes)
}
