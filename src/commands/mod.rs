pub mod apk;
pub mod setup;
pub mod start;

pub struct GlobalContext {
    pub yes: bool,
}

pub trait Command {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()>;
}

#[derive(clap::Parser)]
pub enum MainCommand {
    /// Initial setup for Quest emulator. Downloads sdkmanager, emulator, system images, etc.
    Setup(setup::SetupArgs),
    /// Start the Android Emulator with a specified AVD
    Start(start::StartArgs),
    /// Commands for patching APKs
    Apk(apk::ApkArgs),
}

impl Command for MainCommand {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()> {
        match self {
            MainCommand::Setup(args) => args.execute(ctx)?,
            MainCommand::Apk(args) => args.execute(ctx)?,
            MainCommand::Start(args) => args.execute(ctx)?,
        }

        Ok(())
    }
}
