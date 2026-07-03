mod enrich;

#[cfg(not(test))]
use std::collections::BTreeMap;
#[cfg(not(test))]
use zellij_tile::prelude::*;

/// Directory (inside the plugin's WASI sandbox) where per-cwd Claude session
/// markers live. The sandbox maps guest `/tmp` to the host's `ZELLIJ_TMP_DIR`
/// (`/tmp/zellij-<uid>`), so a `SessionStart` hook on the host must write to
/// `/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session` for these to be found.
/// `/tmp` is preopened unconditionally, so no `FullHdAccess` permission is needed.
const MARKER_DIR: &str = "/tmp/claude-sessions";

/// Plugin-config defaults, populated in `load()` from the layout's `plugin { … }`
/// block. Per-invocation `--args` on `zellij pipe` override these (see `pipe`).
#[cfg(not(test))]
struct State {
    /// Default for auto-enter when a pipe message doesn't set `auto_enter`.
    auto_enter_default: bool,
    /// Command basename treated as Claude (default "claude").
    claude_command: String,
}

#[cfg(not(test))]
impl Default for State {
    fn default() -> Self {
        Self {
            auto_enter_default: true,
            claude_command: "claude".to_string(),
        }
    }
}

#[cfg(not(test))]
fn parse_truthy(v: &str) -> bool {
    !matches!(v.trim(), "false" | "0" | "no" | "off")
}

#[cfg(not(test))]
impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        // Optional defaults from the resident-plugin layout: `plugin location="…" {
        // auto_enter "false"; claude_command "claude-code" }`. Per-snapshot `--args`
        // still override these.
        if let Some(v) = configuration.get("auto_enter") {
            self.auto_enter_default = parse_truthy(v);
        }
        if let Some(v) = configuration
            .get("claude_command")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            self.claude_command = v.to_string();
        }
        // dump_session_layout / save_layout are synchronous request/response calls
        // (they block on host_run_plugin_command), so no event subscription is needed.
        request_permission(&[
            PermissionType::ReadApplicationState,   // DumpSessionLayout
            PermissionType::ChangeApplicationState, // SaveLayout
        ]);
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if pipe_message.name == "save" {
            let name = pipe_message
                .payload
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "unnamed".to_string());

            // Auto-enter (drop `start_suspended` on the claude panes we pin, so they
            // resume without a manual ENTER) is ON by default — that's the whole point
            // of the tool. Per-snapshot override: `--args auto_enter=false`.
            let auto_enter = pipe_message
                .args
                .get("auto_enter")
                .map(|v| parse_truthy(v))
                .unwrap_or(self.auto_enter_default);

            // Command basename to treat as Claude — from `--args claude_command=…`,
            // else the load()-config default, else "claude".
            let claude_command = pipe_message
                .args
                .get("claude_command")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or(self.claude_command.as_str());

            let cfg = enrich::EnrichConfig {
                auto_enter,
                claude_command,
            };

            // dump_session_layout() returns the layout KDL synchronously — the result
            // IS the layout; it does NOT arrive later as a CustomMessage event.
            match dump_session_layout() {
                Ok((kdl, _metadata)) => {
                    let (enriched, stats) =
                        enrich::enrich_layout(&kdl, &|cwd: &str| resolve_session_uuid(cwd), &cfg);
                    match save_layout(&name, &enriched, true) {
                        Ok(()) => {
                            eprintln!(
                                "[zellij-claude-restore] saved '{}' — {} enriched, {} already pinned, {} missing marker",
                                name, stats.enriched, stats.already_pinned, stats.missing_marker
                            );
                            write_status(&name, &stats, true);
                        }
                        Err(e) => {
                            eprintln!(
                                "[zellij-claude-restore] save_layout('{}') failed: {}",
                                name, e
                            );
                            write_status(&name, &stats, false);
                        }
                    }
                }
                Err(e) => eprintln!("[zellij-claude-restore] dump_session_layout failed: {}", e),
            }
            // NOTE: the save above is synchronous and complete by this point, but the
            // `zellij pipe` CLI call stays blocked (it waits for plugin output that
            // never comes). unblock_cli_pipe_input() does NOT release it — it unblocks
            // the plugin's input side, not the CLI's wait. The `snap` wrapper works
            // around this with a short `timeout`; the snapshot is already saved.
        }
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {}
}

#[cfg(not(test))]
register_plugin!(State);

/// Write a one-line JSON status file the `snap` shell helper reads to report what
/// actually happened (instead of only checking that the snapshot file exists). Path
/// is guest `/tmp/claude-sessions/.last-save.json` = host
/// `/tmp/zellij-<uid>/claude-sessions/.last-save.json`. Best-effort: any I/O error
/// is ignored — the snapshot save is what matters, feedback is a bonus.
#[cfg(not(test))]
fn write_status(name: &str, stats: &enrich::EnrichStats, ok: bool) {
    let json = format!(
        "{{\"ok\":{},\"name\":{:?},\"enriched\":{},\"already_pinned\":{},\"missing_marker\":{},\"parse_failed\":{}}}\n",
        ok, name, stats.enriched, stats.already_pinned, stats.missing_marker, stats.parse_failed
    );
    let path = format!("{}/.last-save.json", MARKER_DIR);
    let _ = std::fs::create_dir_all(MARKER_DIR);
    let _ = std::fs::write(&path, json);
}

/// Read the Claude session UUID for a given absolute cwd from its marker file.
/// Marker key = cwd with `/` replaced by `-` (matching Claude's own
/// `~/.claude/projects/<encoded-cwd>/` convention).
#[cfg_attr(test, allow(dead_code))]
fn resolve_session_uuid(cwd: &str) -> Option<String> {
    let encoded = cwd.replace('/', "-");
    let path = format!("{}/{}.session", MARKER_DIR, encoded);
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
