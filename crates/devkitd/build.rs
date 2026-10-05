use std::path::Path;
use std::process::Command;

/// `rust-embed` needs `ui/dist` to exist at compile time. Rather than make
/// every contributor remember a separate `npm run build` step, this
/// builds it automatically — the whole point of the tool is "one binary,
/// no setup".
fn main() {
    let ui_dir = Path::new("../../ui");
    println!("cargo:rerun-if-changed={}", ui_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", ui_dir.join("index.html").display());
    println!("cargo:rerun-if-changed={}", ui_dir.join("package.json").display());

    if ui_dir.join("dist").join("index.html").exists() {
        return;
    }

    if !ui_dir.join("node_modules").exists() {
        run(ui_dir, "npm", &["install"]);
    }
    run(ui_dir, "npm", &["run", "build"]);
}

fn run(cwd: &Path, cmd: &str, args: &[&str]) {
    let status = Command::new(cmd).args(args).current_dir(cwd).status().unwrap_or_else(|e| {
        panic!(
            "failed to run `{cmd} {}` in {}: {e}\n\
             Install Node.js, or build the UI yourself first: \
             `npm --prefix ui install && npm --prefix ui run build`",
            args.join(" "),
            cwd.display()
        )
    });
    if !status.success() {
        panic!("`{cmd} {}` exited with {status} in {}", args.join(" "), cwd.display());
    }
}
