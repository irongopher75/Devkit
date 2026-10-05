use std::path::Path;

use serde_json::Value as JsonValue;

use crate::config::CommandDef;

/// Runs every auto-detector and concatenates the results. Order matters
/// only for display order in the UI; `deck.toml`, when present, bypasses
/// all of this (see `config::load_or_detect`).
pub fn detect_all(project_dir: &Path) -> Vec<CommandDef> {
    let mut commands = Vec::new();
    commands.extend(detect_package_json(project_dir));
    commands.extend(detect_docker_compose(project_dir));
    commands.extend(detect_procfile(project_dir));
    commands
}

fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

pub fn detect_package_json(project_dir: &Path) -> Vec<CommandDef> {
    let path = project_dir.join("package.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<JsonValue>(&text) else {
        tracing::warn!("package.json at {} is not valid JSON; skipping", path.display());
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) else {
        return Vec::new();
    };

    scripts
        .iter()
        .filter_map(|(name, script)| {
            // Only confirms this script entry is a plain string (npm also
            // allows objects in some configs); the command we actually run
            // is always `npm run <name>`, which lets npm resolve it.
            script.as_str()?;
            Some(CommandDef {
                id: format!("npm-{}", slugify(name)),
                name: format!("npm run {name}"),
                cmd: format!("npm run {name}"),
                cwd: None,
                env: Default::default(),
                auto_start: false,
                source: "package.json".into(),
            })
        })
        .collect()
}

pub fn detect_docker_compose(project_dir: &Path) -> Vec<CommandDef> {
    let candidates = ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"];
    let Some(path) = candidates.iter().map(|f| project_dir.join(f)).find(|p| p.exists()) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
        tracing::warn!("{} is not valid YAML; skipping", path.display());
        return Vec::new();
    };
    let Some(services) = doc.get("services").and_then(|s| s.as_mapping()) else {
        return Vec::new();
    };

    services
        .keys()
        .filter_map(|k| k.as_str())
        .map(|service| CommandDef {
            id: format!("compose-{}", slugify(service)),
            name: format!("{service} (compose)"),
            cmd: format!("docker compose up {service}"),
            cwd: None,
            env: Default::default(),
            auto_start: false,
            source: "docker-compose.yml".into(),
        })
        .collect()
}

pub fn detect_procfile(project_dir: &Path) -> Vec<CommandDef> {
    let path = project_dir.join("Procfile");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, cmd) = line.split_once(':')?;
            let name = name.trim();
            let cmd = cmd.trim();
            if name.is_empty() || cmd.is_empty() {
                return None;
            }
            Some(CommandDef {
                id: format!("proc-{}", slugify(name)),
                name: name.to_string(),
                cmd: cmd.to_string(),
                cwd: None,
                env: Default::default(),
                auto_start: false,
                source: "Procfile".into(),
            })
        })
        .collect()
}
