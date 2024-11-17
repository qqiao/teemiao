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
use log::{debug, error, info, trace};
use serde::Serialize;
use std::path::{absolute, Path, PathBuf};

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

impl From<gix::open::Error> for TeemiaoError {
    fn from(err: gix::open::Error) -> TeemiaoError {
        TeemiaoError {
            message: format!("Failed to open repository: {}", err),
        }
    }
}

impl From<gix::reference::find::existing::Error> for TeemiaoError {
    fn from(err: gix::reference::find::existing::Error) -> TeemiaoError {
        TeemiaoError {
            message: format!("Failed to find reference: {}", err),
        }
    }
}

impl From<gix::id::shorten::Error> for TeemiaoError {
    fn from(err: gix::id::shorten::Error) -> TeemiaoError {
        TeemiaoError {
            message: format!("Failed to get short revision: {}", err),
        }
    }
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
        let out = absolute(out).unwrap().canonicalize().unwrap();
        debug!("Output file: {}", &out.display());

        trace!("Getting current directory");
        let p = match Path::new(".").canonicalize() {
            Ok(p) => match absolute(p) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to get current directory: {}", e);
                    return Err(TeemiaoError::from(e));
                }
            },
            Err(e) => {
                error!("Failed to get current directory: {}", e);
                return Err(TeemiaoError::from(e));
            }
        };
        trace!("Current directory: {}", p.display());

        trace!("Opening git repository");
        let repo = match gix::open(p) {
            Ok(repo) => repo,
            Err(e) => {
                error!("Failed to open repository: {}", e);
                return Err(TeemiaoError::from(e));
            }
        };
        trace!("Repository opened successfully");

        trace!("Getting head");
        let head = match repo.head() {
            Ok(head) => head,
            Err(e) => {
                error!("Failed to get head: {}", e);
                return Err(TeemiaoError::from(e));
            }
        };
        trace!("Head obtained successfully");

        trace!("Getting short revision");
        let revision = match head.id() {
            Some(revision) => match revision.shorten() {
                Ok(revision) => revision.to_string(),
                Err(e) => {
                    error!("Failed to get short revision: {}", e);
                    return Err(TeemiaoError::from(e));
                }
            },
            None => {
                error!("Failed to get short revision");
                return Err(TeemiaoError {
                    message: "Failed to get short revision".to_string(),
                });
            }
        };
        trace!("Short revision obtained successfully");

        let build_info = BuildInfo {
            revision,
            build_time: chrono::Utc::now().timestamp(),
        };
        trace!("Build info created successfully");

        trace!("Writing to file");
        // write to file
        let file = std::fs::File::create(out.clone())?;
        serde_json::to_writer_pretty(file, &build_info)?;
        info!("Build info written successfully to {}", &out.display());

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
