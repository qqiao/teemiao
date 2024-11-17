// Copyright 2024 Qian Qiao
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Build information related functionalities.

use crate::TeemiaoError;
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

/// Automatically generates structured metadata about your build process in JSON
/// format.
///
/// The metadata includes the build time and the current git revision of the
/// code base.
///
/// In the longer term, we would like to support VCS(s) other than just git,
/// but as of now, this isn't a priority.
#[derive(Debug, Args)]
#[command()]
pub struct BuildInfoCommand {
    /// Output file
    #[arg(default_value = "./build_info.json", value_name = "FILE")]
    out: Option<PathBuf>,
}

/// Build information data structure.
#[derive(Serialize)]
pub struct BuildInfo {
    /// Revision is the current git revision of the code base.
    revision: String,

    /// Build time is the timestamp of the build.
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
            .args(["rev-parse", "--short", "HEAD"])
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
