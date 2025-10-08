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

use clap::Args;
use log::{debug, error, info, trace};
use serde::Serialize;
use std::path::{absolute, PathBuf};
use thiserror::Error;

/// Errors that can occur while generating build information.
#[derive(Debug, Error)]
pub enum BuildInfoError {
    /// An error occurred while opening the repository.
    #[error("Failed to open repository: {0}")]
    OpenRepository(#[from] gix::open::Error),

    /// An error occurred while finding the reference.
    #[error("Failed to find reference: {0}")]
    FindReference(#[from] gix::reference::find::existing::Error),

    /// An error occurred while shortening the revision.
    #[error("Failed to get short revision: {0}")]
    ShortenRevision(#[from] gix::id::shorten::Error),

    /// The HEAD reference does not point to a valid commit ID.
    #[error("Head ID not found")]
    HeadIdNotFound,

    /// An error occurred while writing to the output file.
    #[error("Failed to write to output file: {0}")]
    WriteOutput(#[from] std::io::Error),

    /// An error occurred while serializing the build info.
    #[error("Failed to serialize build info: {0}")]
    Serialize(#[from] serde_json::Error),
}

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
    /// Path to the output file where build information will be written.
    ///
    /// If not specified, defaults to `./build_info.json` in the current
    /// working directory.
    #[arg(default_value = "./build_info.json", value_name = "FILE")]
    out: Option<PathBuf>,
}

/// Build information data structure.
///
/// Contains metadata about the build process including the git revision
/// and the timestamp when the build was created.
#[derive(Debug, Serialize)]
pub struct BuildInfo {
    /// The current git revision (short hash) of the code base.
    revision: String,

    /// The Unix timestamp (seconds since epoch) when the build was created.
    build_time: i64,
}

impl BuildInfoCommand {
    /// Executes the build information generation command.
    ///
    /// This method gathers build metadata from the current git repository
    /// and writes it to the specified output file in JSON format.
    ///
    /// # Errors
    ///
    /// Returns a `BuildInfoError` if:
    /// - The current directory cannot be determined or accessed
    /// - The git repository cannot be opened
    /// - The HEAD reference cannot be retrieved
    /// - The output file cannot be created or written to
    /// - JSON serialization fails
    #[allow(clippy::result_large_err)]
    pub fn run(&self) -> Result<(), BuildInfoError> {
        info!("Generating build info...");

        trace!("Getting current working directory...");
        let cwd = std::env::current_dir()?;
        let cwd = absolute(cwd)?.canonicalize()?;
        trace!("Current working directory: {}", cwd.display());

        trace!("Determining output file...");
        // if out is not set, default to ${cwd}/build_info.json
        let out = self.out.clone().unwrap_or_else(|| {
            let mut path = cwd.clone();
            path.push("build_info.json");
            path
        });
        let out = absolute(out)?;
        debug!("Output file: {}", &out.display());

        trace!("Opening {} as git repository...", &cwd.display());
        let repo = gix::open(&cwd)?;
        trace!("Repository opened successfully");

        trace!("Getting head revision...");
        let head = repo.head()?;
        trace!("Head obtained successfully: {:?}", head.id());

        trace!("Getting short revision for {:?}...", head.id());
        let revision = head
            .id()
            .ok_or(BuildInfoError::HeadIdNotFound)?
            .shorten()?
            .to_string();
        trace!("Short revision obtained successfully: {}", revision);

        let build_info = BuildInfo {
            revision,
            build_time: chrono::Utc::now().timestamp(),
        };
        trace!("Build info created successfully: {:?}", build_info);

        trace!("Writing build info to {}...", &out.display());
        // write to file
        let file = std::fs::File::create(out.clone())?;
        serde_json::to_writer_pretty(file, &build_info)?;
        info!("Build info successfully written to {}", &out.display());

        Ok(())
    }
}
