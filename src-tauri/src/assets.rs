use crate::config::AssetConfig;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
use tokio::{io::AsyncWriteExt, process::Command};

/// Install one reviewed native asset after size, hash, and archive-path validation.
pub async fn install(data_dir: &Path, asset: &AssetConfig) -> Result<PathBuf, String> {
    let destination = data_dir.join(&asset.install_directory);
    let expected = destination.join(&asset.expected_file);
    if expected.is_file() {
        return Ok(expected);
    }
    let downloads = data_dir.join("downloads");
    fs::create_dir_all(&downloads).map_err(err)?;
    fs::create_dir_all(&destination).map_err(err)?;
    let archive = downloads.join(&asset.filename);
    if !archive.is_file() || hash(&archive)? != asset.sha256 {
        let partial = archive.with_extension("partial");
        let response = reqwest::get(&asset.url).await.map_err(err)?;
        if !response.status().is_success() {
            return Err(format!("Download returned {}", response.status()));
        }
        if response.content_length().unwrap_or(asset.bytes) > asset.bytes {
            return Err("Download is larger than the reviewed asset".into());
        }
        let mut file = tokio::fs::File::create(&partial).await.map_err(err)?;
        let mut total = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(err)?;
            total += bytes.len() as u64;
            if total > asset.bytes {
                return Err("Download exceeded its reviewed size".into());
            }
            file.write_all(&bytes).await.map_err(err)?;
        }
        file.flush().await.map_err(err)?;
        drop(file);
        if hash(&partial)? != asset.sha256 {
            let _ = fs::remove_file(&partial);
            return Err("Downloaded runtime failed SHA-256 verification".into());
        }
        fs::rename(partial, &archive).map_err(err)?;
    }
    let listing = Command::new("tar.exe")
        .args(["-tf", &archive.to_string_lossy()])
        .output()
        .await
        .map_err(err)?;
    if !listing.status.success() {
        return Err("Could not inspect the runtime archive".into());
    }
    let names = String::from_utf8_lossy(&listing.stdout);
    if names.lines().count() > 5000
        || names.lines().any(|name| {
            Path::new(name).is_absolute()
                || Path::new(name)
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        })
    {
        return Err("Runtime archive contains unsafe paths".into());
    }
    let detailed = Command::new("tar.exe")
        .args(["-tvf", &archive.to_string_lossy()])
        .output()
        .await
        .map_err(err)?;
    if !detailed.status.success()
        || String::from_utf8_lossy(&detailed.stdout)
            .lines()
            .any(|line| !matches!(line.chars().next(), Some('-' | 'd')))
    {
        return Err("Runtime archive contains unsupported entries or links".into());
    }
    let status = Command::new("tar.exe")
        .args([
            "-xf",
            &archive.to_string_lossy(),
            "-C",
            &destination.to_string_lossy(),
        ])
        .status()
        .await
        .map_err(err)?;
    if !status.success() || !expected.is_file() {
        return Err("Runtime archive did not contain llama-server.exe".into());
    }
    Ok(expected)
}

fn hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(err)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(err)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
