use clap::Args;

use crate::TeemiaoError;

#[derive(Debug, Args)]
#[command()]
pub struct BuildInfo {}

impl BuildInfo {
    pub fn generate(&self) -> Result<(), TeemiaoError> {
        Ok(())
    }
}
