use std::{fs::File, io::BufReader, path::PathBuf};

use color_eyre::eyre::Context;

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

impl Command for ApkArgs {
    fn execute(self, _ctx: &crate::commands::GlobalContext) -> color_eyre::Result<()> {
        match self.action {
            ApkAction::Patch { path } => {
                println!("Patching APK from path: {:?}", path);
                let apk_file = File::open(&path).context("")?;
                let apk_file = BufReader::new(apk_file);
                let mut apk = mbf_zip::ZipFile::open(apk_file)
                    .map_err(|a| color_eyre::eyre::eyre!(a))
                    .context("Failed to read APK as zip file")?;

                let manifest = apk.read_file("AndroidManifest.xml")
                    .map_err(|a| color_eyre::eyre::eyre!(a))
                    .context("Failed to read AndroidManifest.xml from APK")?;

                todo!("Parse and modify the AndroidManifest.xml to add required features");
            }
        }
        Ok(())
    }
}
