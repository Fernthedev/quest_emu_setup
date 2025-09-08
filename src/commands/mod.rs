pub mod setup;

pub struct GlobalContext {
    pub yes: bool,
}

pub trait Command {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()>;
}

#[derive(clap::Parser)]
pub enum MainCommand {
    Setup(setup::SetupArgs),
}

impl Command for MainCommand {
    fn execute(self, ctx: &GlobalContext) -> color_eyre::Result<()> {
        match self {
            MainCommand::Setup(args) => args.execute(ctx)?,
        }

        Ok(())
    }
}
