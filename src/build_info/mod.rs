//! Build information related functionalities.

use clap::Args;

use crate::TeemiaoError;

/// Build information command
#[derive(Debug, Args)]
#[command()]
pub struct BuildInfoCommand {}

impl BuildInfoCommand {
    /// Run the build information command.
    pub fn run(&self) -> Result<(), TeemiaoError> {
        todo!("Implement this function");
    }
}
