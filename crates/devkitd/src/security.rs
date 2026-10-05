use std::path::Path;

use rand::RngCore;

/// Header the UI must send on every mutating API call and on the WebSocket
/// upgrade. Not a cookie: cookies ride along automatically on cross-origin
/// requests, a custom header does not, so this alone defeats a plain CSRF
/// form post from an attacker's page.
pub const TOKEN_HEADER: &str = "x-devkit-token";

/// A fresh, unguessable token per daemon run. Never persisted across runs,
/// never logged anywhere a shell history or CI log could pick it up.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Writes `.devkit/session` with the port and token, mode 0600, so a human
/// (or a script they trust) can `curl` the API without reading daemon
/// stdout. The directory and file are gitignored by the scaffold this tool
/// generates in new projects.
pub fn persist_session(project_dir: &Path, port: u16, token: &str) -> anyhow::Result<()> {
    let dir = project_dir.join(".devkit");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("session");
    std::fs::write(&path, format!("port={port}\ntoken={token}\n"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// Allowed Origins for a daemon bound to 127.0.0.1 on `port`. Both
/// loopback spellings are accepted since browsers treat them as distinct
/// origins.
fn allowed_origins(port: u16) -> [String; 2] {
    [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}")]
}

/// Blocks DNS rebinding: an attacker page served from a real domain that
/// later resolves to 127.0.0.1 still carries its *original* Origin header
/// (browsers derive Origin from the page's own URL, not from where the
/// hostname resolves), so it never matches our loopback origins.
pub fn is_allowed_origin(origin: Option<&str>, port: u16) -> bool {
    match origin {
        Some(origin) => allowed_origins(port).contains(&origin.to_string()),
        // No Origin header at all (e.g. a non-browser tool hitting the API
        // directly over loopback) is allowed through on Origin alone; the
        // token check is what actually gates it.
        None => true,
    }
}

pub fn tokens_match(provided: Option<&str>, expected: &str) -> bool {
    provided.map(|p| constant_time_eq(p.as_bytes(), expected.as_bytes())).unwrap_or(false)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
