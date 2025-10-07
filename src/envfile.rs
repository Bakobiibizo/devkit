use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

/// Lightweight representation of a `.env` file.
#[derive(Debug)]
pub struct EnvFile {
    path: Utf8PathBuf,
}

impl EnvFile {
    pub fn open(path: &Utf8Path) -> Result<Self> {
        Ok(Self {
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }
}

pub fn locate(start: &Utf8Path) -> Result<Utf8PathBuf> {
    let _ = start;
    // TODO: walk up directories to find a `.env` file or create one.
    Ok(Utf8PathBuf::new())
}
