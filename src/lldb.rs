use std::path::{Path, PathBuf};

use color_eyre::eyre::Context;

#[derive(serde::Serialize)]
pub struct DapAttachConfig {
    request: &'static str,
    pid: u32,
    #[serde(rename = "attachCommands")]
    attach_commands: Vec<String>,
}

/// The lldb-dap attach config/URI for a running process, plus its pid.
pub struct AttachInfo {
    pub pid: u32,
    pub config_json: String,
    pub debug_uri: String,
}

/// Resolves a symbols path into a list of files: passed through as-is if it's a
/// file, or recursively searched for `.so` files if it's a directory.
fn collect_so_files(path: &Path, out: &mut Vec<PathBuf>) -> color_eyre::Result<()> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path)
            .with_context(|| format!("Failed to read symbols directory {}", path.display()))?
        {
            let entry_path = entry?.path();
            if entry_path.is_dir() {
                collect_so_files(&entry_path, out)?;
            } else if entry_path.extension().is_some_and(|ext| ext == "so") {
                out.push(entry_path);
            }
        }
    } else {
        out.push(path.to_path_buf());
    }
    Ok(())
}

/// Builds the lldb-dap attach config/URI for `pid`, listening on the lldb-server
/// started by `start_lldb_server` on `port`. lldb-dap's `attachCommands`, when
/// present, take the place of its built-in attach-by-pid logic entirely, so the
/// platform select/connect and the actual `attach` all have to happen inside it.
/// Port forwarding maps this host's tcp:port to the same port on the device, so
/// lldb should connect to localhost, not the device's adb serial.
pub fn build_attach_info(
    pid: &str,
    port: u16,
    symbols: &[PathBuf],
) -> color_eyre::Result<AttachInfo> {
    let mut symbol_files = Vec::new();
    for path in symbols {
        collect_so_files(path, &mut symbol_files)
            .with_context(|| format!("Failed to resolve symbols path {}", path.display()))?;
    }

    let mut attach_commands = vec![
        "platform select remote-android".to_string(),
        "settings set target.inherit-env false".to_string(),
        format!("platform connect connect://localhost:{port}"),
        format!("attach -p {pid}"),
        "pro hand -p true -s false SIGPWR".to_string(),
        "pro hand -p true -s false SIGXCPU".to_string(),
        "pro hand -p true -s false SIG33".to_string(),
    ];
    attach_commands.extend(
        symbol_files
            .iter()
            .map(|p| format!("target symbols add {}", p.display())),
    );

    let config = DapAttachConfig {
        request: "attach",
        pid: pid
            .parse()
            .with_context(|| format!("adb reported a non-numeric pid: {pid}"))?,
        attach_commands,
    };
    let config_json =
        serde_json::to_string(&config).context("Failed to serialize lldb-dap attach config")?;
    let debug_uri = format!(
        "vscode://llvm-vs-code-extensions.lldb-dap/start?config={}",
        percent_encoding::utf8_percent_encode(&config_json, percent_encoding::NON_ALPHANUMERIC)
    );

    Ok(AttachInfo {
        pid: config.pid,
        config_json,
        debug_uri,
    })
}
