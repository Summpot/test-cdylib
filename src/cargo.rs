use cargo_metadata::Message;
use serde::Deserialize;
use serde_json::Map;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use crate::error::{Error, Result};
use crate::features;
use crate::run::Project;
use crate::rustflags;

#[derive(Deserialize)]
pub struct Metadata {
    pub target_directory: PathBuf,
    pub workspace_root: PathBuf,
}

fn raw_cargo() -> Command {
    Command::new(option_env!("CARGO").unwrap_or("cargo"))
}

fn cargo_supports_offline() -> bool {
    raw_cargo()
        .arg("--version")
        .arg("--offline")
        .output()
        .ok()
        .map_or(false, |res| res.status.success())
}

fn cargo_build(features: &Option<Vec<String>>) -> Command {
    let mut cmd = raw_cargo();
    if cargo_supports_offline() {
        cmd.arg("--offline");
    }
    cmd.arg("build")
        .arg("--message-format=json")
        .args(feature_args(features));
    rustflags::set_env(&mut cmd);
    cmd
}

pub fn build_cdylib(project: &Project) -> Result<PathBuf> {
    parse_output(
        cargo_build(&project.features)
            .current_dir(&project.dir)
            .env("CARGO_TARGET_DIR", path!(&project.dir / "target"))
            .stderr(Stdio::inherit())
            .output()
            .map_err(Error::Cargo)?,
    )
}

pub fn build_self_cdylib() -> Result<PathBuf> {
    parse_output(
        cargo_build(&features::find())
            .arg("--lib")
            .stderr(Stdio::inherit())
            .output()
            .map_err(Error::Cargo)?,
    )
}

pub fn build_example(name: &str) -> Result<PathBuf> {
    parse_output(
        cargo_build(&features::find())
            .arg("--example")
            .arg(name)
            .stderr(Stdio::inherit())
            .output()
            .map_err(Error::Cargo)?,
    )
}

pub fn parse_output(result: Output) -> Result<PathBuf> {
    let mut artifact = None;
    for message in Message::parse_stream(Cursor::new(result.stdout)) {
        match message? {
            Message::CompilerMessage(m) => eprintln!("{}", m),
            Message::CompilerArtifact(a) => artifact = Some(a),
            _ => (),
        }
    }

    if !result.status.success() {
        return Err(Error::CargoFail);
    }
    match artifact {
        Some(artifact) => artifact
            .filenames
            .into_iter()
            .filter(|filename| match filename.extension() {
                Some("dll") | Some("dylib") | Some("so") => true,
                _ => false,
            })
            .next()
            .ok_or(Error::CdylibNotFound)
            .map(|a| a.into_std_path_buf()),
        None => Err(Error::CargoFail),
    }
}

pub fn metadata() -> Result<Metadata> {
    let output = raw_cargo()
        .arg("metadata")
        .arg("--format-version=1")
        .output()
        .map_err(Error::Cargo)?;

    serde_json::from_slice(&output.stdout).map_err(Error::Metadata)
}

fn feature_args(features: &Option<Vec<String>>) -> Vec<String> {
    match features {
        Some(features) => vec![
            "--no-default-features".to_owned(),
            "--features".to_owned(),
            features.join(","),
        ],
        None => Vec::new(),
    }
}
