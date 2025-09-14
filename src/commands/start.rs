use std::process;

use color_eyre::eyre::{Context, bail};
use itertools::Itertools;

use crate::{commands::Command, constants};

#[derive(clap::Parser, Debug)]
pub struct StartArgs {
    /// Name of the AVD to start
    #[arg(long, default_value_t = constants::DEFAULT_AVD_NAME.to_string())]
    pub name: String,

    /// Start the emulator without loading a snapshot
    #[arg(long, default_value_t = false)]
    pub fresh: bool,

    /// Additional arguments to pass to the emulator
    #[arg(last = true)]
    pub args: Vec<String>,
}

impl Command for StartArgs {
    fn execute(self, _ctx: &crate::commands::GlobalContext) -> color_eyre::Result<()> {
        // use emulator @android13desktop -selinux permissive -feature QtRawKeyboardInput

        println!("Starting emulator with AVD name: {}", self.name);

        let mut command = process::Command::new("emulator");
        command
            .arg(format!("@{}", self.name))
            .arg("-selinux")
            .arg("permissive")
            .arg("-feature")
            .arg("QtRawKeyboardInput");

        if self.fresh {
            command.arg("-no-snapshot-load");
        }

        if !self.args.is_empty() {
            command.args(self.args);
        }

        println!("emulator {}", command.get_args().map(|s| s.display()).join(" "));

        let status = command.status().context("Failed to start emulator")?;

        if !status.success() {
            bail!("Emulator exited with status: {}", status);
        }

        Ok(())
    }
}
