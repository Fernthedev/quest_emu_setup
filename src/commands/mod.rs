pub mod setup;
pub mod apk;

pub struct GlobalContext {
    pub yes: bool,
}

pub trait Command {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()>;
}

#[derive(clap::Parser)]
pub enum MainCommand {
    Setup(setup::SetupArgs),
    Apk(apk::ApkArgs),
}

impl Command for MainCommand {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()> {
        match self {
            MainCommand::Setup(args) => args.execute(ctx)?,
            MainCommand::Apk(args) => args.execute(ctx)?,
        }

        Ok(())
    }
}
