use kdl::{KdlDocument, KdlEntry, KdlNode};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;

/// Directory (inside the plugin's WASI sandbox) where per-cwd Claude session
/// markers live. The sandbox maps guest `/tmp` to the host's `ZELLIJ_TMP_DIR`
/// (`/tmp/zellij-<uid>`), so a `SessionStart` hook on the host must write to
/// `/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session` for these to be found.
/// `/tmp` is preopened unconditionally, so no `FullHdAccess` permission is needed.
const MARKER_DIR: &str = "/tmp/claude-sessions";

#[derive(Default)]
struct State {}

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
                    let enriched = enrich_claude_panes(&kdl);
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

register_plugin!(State);

/// Parse the dumped layout KDL, inject `args "--session-id" "<uuid>"` into every
/// restorable `claude` pane whose session UUID can be resolved, and return the
/// re-serialized KDL. On any parse failure the original KDL is returned unchanged
/// so the snapshot is still saved (just without enrichment).
fn enrich_claude_panes(kdl: &str) -> String {
    // zellij dumps KDL v1 syntax (it uses the kdl v4 crate), so parse and
    // re-serialize as v1 — a v2 round-trip would produce a layout zellij can't read.
    let mut doc = match KdlDocument::parse_v1(kdl) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("[zellij-claude-sync] KDL parse failed, saving raw layout: {}", e);
            return kdl.to_string();
        }
    };
    enrich_nodes(doc.nodes_mut(), None, false);
    doc.ensure_v1();
    doc.to_string()
}

/// Names whose subtrees are *templates*, not captured session state. Panes inside
/// them describe what to spawn for a brand-new tab, so they must NOT be pinned to
/// a specific (old) session id.
fn is_template_node(name: &str) -> bool {
    matches!(
        name,
        "new_tab_template" | "tab_template" | "swap_tiled_layout" | "swap_floating_layout"
    )
}

/// Recursively walk the layout tree, enriching `claude` panes that hold live state.
/// `inherited_base` is the nearest ancestor `cwd "…"` value, used to resolve
/// relative pane `cwd` properties to absolute paths.
fn enrich_nodes(nodes: &mut Vec<KdlNode>, inherited_base: Option<String>, in_template: bool) {
    // A `cwd "…"` child node sets the base cwd for this scope (e.g. the top-level
    // `layout { cwd "/home/user" … }`). Pane `cwd` properties are relative to it.
    let scope_base = nodes
        .iter()
        .find(|n| n.name().value() == "cwd")
        .and_then(|n| n.entries().first())
        .and_then(|e| e.value().as_string())
        .map(|s| s.to_string())
        .or(inherited_base);

    for node in nodes.iter_mut() {
        let name = node.name().value().to_string();
        let entering_template = in_template || is_template_node(&name);

        if !entering_template && name == "pane" {
            maybe_enrich_pane(node, scope_base.as_deref());
        }

        if let Some(children) = node.children_mut().as_mut() {
            enrich_nodes(children.nodes_mut(), scope_base.clone(), entering_template);
        }
    }
}

fn maybe_enrich_pane(node: &mut KdlNode, base_cwd: Option<&str>) {
    let is_claude = node
        .entry("command")
        .and_then(|e| e.value().as_string())
        .map(|cmd| basename(cmd) == "claude")
        .unwrap_or(false);
    if !is_claude {
        return;
    }

    // Don't clobber a pane that already carries an explicit --session-id.
    if pane_has_session_id(node) {
        return;
    }

    let pane_cwd = node.entry("cwd").and_then(|e| e.value().as_string());
    let full_cwd = match resolve_cwd(pane_cwd, base_cwd) {
        Some(cwd) => cwd,
        None => {
            eprintln!("[zellij-claude-sync] claude pane has no resolvable cwd, leaving bare");
            return;
        }
    };

    match resolve_session_uuid(&full_cwd) {
        Some(uuid) => {
            inject_session_id(node, &uuid);
            eprintln!(
                "[zellij-claude-sync] enriched claude pane (cwd={}) with session {}",
                full_cwd, uuid
            );
        }
        None => eprintln!(
            "[zellij-claude-sync] no session marker for cwd={}, leaving bare",
            full_cwd
        ),
    }
}

/// Resolve a pane's `cwd` property to an absolute path. Absolute values pass
/// through; relative values are joined onto the inherited base cwd; a pane with no
/// `cwd` inherits the base directly.
fn resolve_cwd(pane_cwd: Option<&str>, base_cwd: Option<&str>) -> Option<String> {
    match pane_cwd {
        Some(cwd) if cwd.starts_with('/') => Some(cwd.to_string()),
        Some(cwd) => base_cwd.map(|base| format!("{}/{}", base.trim_end_matches('/'), cwd)),
        None => base_cwd.map(|base| base.to_string()),
    }
}

fn pane_has_session_id(node: &KdlNode) -> bool {
    node.children()
        .map(|doc| {
            doc.nodes().iter().any(|n| {
                n.name().value() == "args"
                    && n.entries()
                        .iter()
                        .any(|e| e.value().as_string() == Some("--session-id"))
            })
        })
        .unwrap_or(false)
}

fn inject_session_id(node: &mut KdlNode, uuid: &str) {
    let children = node.ensure_children();
    if let Some(args) = children
        .nodes_mut()
        .iter_mut()
        .find(|n| n.name().value() == "args")
    {
        // Prepend so --session-id leads any user-provided args.
        args.entries_mut().insert(0, KdlEntry::new(uuid.to_string()));
        args.entries_mut().insert(0, KdlEntry::new("--session-id"));
    } else {
        let mut args = KdlNode::new("args");
        args.push(KdlEntry::new("--session-id"));
        args.push(KdlEntry::new(uuid.to_string()));
        children.nodes_mut().push(args);
    }
}

/// Read the Claude session UUID for a given absolute cwd from its marker file.
/// Marker key = cwd with `/` replaced by `-` (matching Claude's own
/// `~/.claude/projects/<encoded-cwd>/` convention).
fn resolve_session_uuid(cwd: &str) -> Option<String> {
    let encoded = cwd.replace('/', "-");
    let path = format!("{}/{}.session", MARKER_DIR, encoded);
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
