use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub last_launched: Option<u64>,
}

use bevy::prelude::Resource;

#[derive(Debug, Serialize, Deserialize, Clone, Default, Resource)]
pub struct LauncherConfig {
    pub projects: Vec<Project>,
}

pub fn get_config_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let mut path = PathBuf::from(home);
    path.push(".debevl");
    path.push("config");
    Some(path)
}

pub fn load_config() -> LauncherConfig {
    let Some(mut path) = get_config_path() else {
        return LauncherConfig::default();
    };
    path.push("projects.json");

    if !path.exists() {
        return LauncherConfig::default();
    }

    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => return LauncherConfig::default(),
    };

    let mut contents = String::new();
    if file.read_to_string(&mut contents).is_err() {
        return LauncherConfig::default();
    }

    serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save_config(config: &LauncherConfig) {
    let Some(dir_path) = get_config_path() else {
        return;
    };

    if !dir_path.exists() && create_dir_all(&dir_path).is_err() {
        return;
    }

    let mut path = dir_path.clone();
    path.push("projects.json");

    let Ok(serialized) = serde_json::to_string_pretty(config) else {
        return;
    };

    if let Ok(mut file) = File::create(&path) {
        let _ = file.write_all(serialized.as_bytes());
    }
}
