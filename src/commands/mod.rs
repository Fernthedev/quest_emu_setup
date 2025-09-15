pub mod apk;
pub mod create;
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
    /// Create for Quest emulator. Downloads sdkmanager, emulator, system images, etc.
    Create(create::CreateArgs),
    /// Start the Android Emulator with a specified AVD
    Start(start::StartArgs),
    /// Commands for patching APKs
    Apk(apk::ApkArgs),
    /// Setup the Android SDK, Emulator, and AVD
    Setup(setup::SetupArgs),
}

impl Command for MainCommand {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()> {
        match self {
            MainCommand::Create(args) => args.execute(ctx)?,
            MainCommand::Apk(args) => args.execute(ctx)?,
            MainCommand::Start(args) => args.execute(ctx)?,
            MainCommand::Setup(setup_args) => setup_args.execute(ctx)?,
        }

        Ok(())
    }
}
