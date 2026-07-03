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

#[cfg(not(test))]
#[derive(Default)]
struct State {}

#[cfg(not(test))]
impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
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

            // dump_session_layout() returns the layout KDL synchronously — the result
            // IS the layout; it does NOT arrive later as a CustomMessage event.
            match dump_session_layout() {
                Ok((kdl, _metadata)) => {
                    let enriched =
                        enrich::enrich_claude_panes(&kdl, &|cwd: &str| resolve_session_uuid(cwd));
                    match save_layout(&name, &enriched, true) {
                        Ok(()) => eprintln!("[zellij-claude-sync] saved snapshot '{}'", name),
                        Err(e) => {
                            eprintln!("[zellij-claude-sync] save_layout('{}') failed: {}", name, e)
                        }
                    }
                }
                Err(e) => eprintln!("[zellij-claude-sync] dump_session_layout failed: {}", e),
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
