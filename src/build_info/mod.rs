//! Build information related functionalities.

use std::path::PathBuf;

use clap::Args;

use crate::TeemiaoError;

use serde::Serialize;

/// Generate build information.
///
/// This command generates a JSON file containing metadata about the current
/// build.
///
/// The metadata includes the build time and the current git revision of the
/// code base.
#[derive(Debug, Args)]
#[command()]
pub struct BuildInfoCommand {
    /// Output file
    #[arg(short, long)]
    out: Option<PathBuf>,
}

/// Build information
#[derive(Serialize)]
pub struct BuildInfo {
    /// Revision
    revision: String,

    /// Build time
    build_time: i64,
}

impl BuildInfoCommand {
    /// Run the build information command.
    pub fn run(&self) -> Result<(), TeemiaoError> {
        // if out is not set, default to ${cwd}/build_info.json
        let out = self.out.clone().unwrap_or_else(|| {
            let mut path = std::env::current_dir().unwrap();
            path.push("build_info.json");
            path
        });
        // get git revision
        let revision = match std::process::Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .output()
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "unknown".to_string(),
        };

        let build_info = BuildInfo {
            revision,
            build_time: chrono::Utc::now().timestamp(),
        };

        // write to file
        let file = std::fs::File::create(out)?;
        serde_json::to_writer_pretty(file, &build_info)?;

        Ok(())
    }
}

impl From<std::io::Error> for TeemiaoError {
    fn from(err: std::io::Error) -> TeemiaoError {
        // Convert std::io::Error to TeemiaoError
        TeemiaoError {
            message: format!("IO error: {}", err),
        }
    }
}

impl From<serde_json::Error> for TeemiaoError {
    fn from(err: serde_json::Error) -> TeemiaoError {
        // Convert serde_json::Error to TeemiaoError
        TeemiaoError {
            message: format!("JSON error: {}", err),
        }
    }
}
