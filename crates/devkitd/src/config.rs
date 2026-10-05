use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One runnable command, shown as a button in the UI. `id` is what the
/// browser is allowed to reference — never a raw shell string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    pub id: String,
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub auto_start: bool,
    /// Where this command was discovered, for display only.
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "deck.toml".into()
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawDeckConfig {
    #[serde(default)]
    command: Vec<RawCommand>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawCommand {
    id: String,
    name: Option<String>,
    cmd: String,
    cwd: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    auto_start: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckConfig {
    pub commands: Vec<CommandDef>,
}

/// Loads `deck.toml` if present; otherwise auto-detects commands from
/// `package.json`, `docker-compose.yml`, and `Procfile`. An explicit
/// `deck.toml` always wins outright — it is the escape hatch for anything
/// auto-detection gets wrong.
pub fn load_or_detect(project_dir: &Path) -> anyhow::Result<DeckConfig> {
    let deck_toml = project_dir.join("deck.toml");
    if deck_toml.exists() {
        let text = std::fs::read_to_string(&deck_toml)?;
        let raw: RawDeckConfig = toml::from_str(&text)?;
        let commands = raw
            .command
            .into_iter()
            .map(|c| CommandDef {
                name: c.name.unwrap_or_else(|| c.id.clone()),
                id: c.id,
                cmd: c.cmd,
                cwd: c.cwd,
                env: c.env,
                auto_start: c.auto_start,
                source: "deck.toml".into(),
            })
            .collect();
        return Ok(DeckConfig { commands });
    }

    let mut commands = crate::detect::detect_all(project_dir);
    dedupe_ids(&mut commands);
    Ok(DeckConfig { commands })
}

/// Auto-detection across package.json, compose, and a Procfile can produce
/// colliding ids (e.g. two "web" services). Rather than silently dropping
/// one, suffix later duplicates so nothing disappears.
fn dedupe_ids(commands: &mut [CommandDef]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for cmd in commands.iter_mut() {
        let count = seen.entry(cmd.id.clone()).or_insert(0);
        if *count > 0 {
            cmd.id = format!("{}-{}", cmd.id, count);
        }
        *count += 1;
    }
}
