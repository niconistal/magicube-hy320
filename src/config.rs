//! Saved projectors (names), the default one, and per-projector setup state.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

pub fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("magicube")
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("magicube")
}

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    /// name or address used when no -p is given
    pub default: Option<String>,
    /// name -> address ("192.168.1.50:5555" or "usb")
    #[serde(default)]
    pub names: BTreeMap<String, String>,
}

impl Config {
    fn path() -> PathBuf { dir().join("config.json") }

    pub fn load() -> Result<Self> {
        match fs::read_to_string(Self::path()) {
            Ok(s) => serde_json::from_str(&s).context("config.json is corrupt"),
            Err(_) => Ok(Self::default()),
        }
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(dir())?;
        fs::write(Self::path(), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Turn a name or address into an address.
    pub fn resolve(&self, t: &str) -> String {
        self.names.get(t).cloned().unwrap_or_else(|| t.to_string())
    }

    pub fn name_of(&self, addr: &str) -> Option<&str> {
        self.names.iter().find(|(_, a)| a.as_str() == addr).map(|(n, _)| n.as_str())
    }
}

/// Which setup steps are done on one projector (keyed by serial number).
#[derive(Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub done: BTreeMap<String, String>,
}

impl State {
    fn path(serial: &str) -> PathBuf { dir().join("state").join(format!("{serial}.json")) }

    pub fn load(serial: &str) -> Self {
        fs::read_to_string(Self::path(serial)).ok()
            .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn mark(&mut self, serial: &str, step: &str, note: &str) -> Result<()> {
        self.done.insert(step.to_string(), note.to_string());
        fs::create_dir_all(dir().join("state"))?;
        fs::write(Self::path(serial), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn clear(&mut self, serial: &str, step: &str) -> Result<()> {
        self.done.remove(step);
        fs::create_dir_all(dir().join("state"))?;
        fs::write(Self::path(serial), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}
