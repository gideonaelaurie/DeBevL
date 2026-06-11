use std::path::{Path, PathBuf};
use std::process::{Child, Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Cargo,
    Executable(PathBuf),
    Invalid,
}

pub fn detect_project_type(path: &Path) -> ProjectType {
    if !path.exists() || !path.is_dir() {
        return ProjectType::Invalid;
    }

    if path.join("Cargo.toml").exists() {
        return ProjectType::Cargo;
    }

    // Look for executable files in the root folder
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = p.metadata() {
                        if metadata.permissions().mode() & 0o111 != 0 {
                            // Avoid files that are scripts or config
                            if let Some(ext) = p.extension() {
                                let ext_str = ext.to_string_lossy().to_lowercase();
                                if ext_str == "sh"
                                    || ext_str == "json"
                                    || ext_str == "toml"
                                    || ext_str == "txt"
                                    || ext_str == "md"
                                    || ext_str == "py"
                                {
                                    continue;
                                }
                            }
                            return ProjectType::Executable(p);
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    if let Some(ext) = p.extension() {
                        if ext.to_string_lossy().to_lowercase() == "exe" {
                            return ProjectType::Executable(p);
                        }
                    }
                }
            }
        }
    }

    ProjectType::Invalid
}

pub fn launch_project(path: &Path) -> Result<Child, String> {
    match detect_project_type(path) {
        ProjectType::Cargo => Command::new("cargo")
            .args(["run", "--release"])
            .current_dir(path)
            .spawn()
            .map_err(|e| format!("Failed to spawn cargo run: {}", e)),
        ProjectType::Executable(exe_path) => Command::new(&exe_path)
            .current_dir(path)
            .spawn()
            .map_err(|e| format!("Failed to spawn executable ({}): {}", exe_path.display(), e)),
        ProjectType::Invalid => Err("No valid Cargo.toml or executable found in this folder. Please verify this is a Bevy game folder.".to_string()),
    }
}
