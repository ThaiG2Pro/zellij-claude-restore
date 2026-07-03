use kdl::{KdlDocument, KdlEntry, KdlNode};

/// Injected session-marker lookup: given an absolute cwd, returns the Claude
/// session UUID for that directory, or `None` if no marker is present.
/// In production (`main.rs`) this is `&|cwd| resolve_session_uuid(cwd)`;
/// in tests it is an inline stub such as `|_| Some("00000000-…".into())`.
pub type SessionResolver<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// What `enrich_layout` did — a summary the caller can surface to the user so a
/// snapshot isn't just a silent "a file appeared". Counts are over the *live*
/// (non-template) `claude` panes only.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EnrichStats {
    /// `claude` panes given a fresh `--resume <uuid>` this run.
    pub enriched: usize,
    /// `claude` panes that already carried `--resume`/`--session-id` (left as-is).
    pub already_pinned: usize,
    /// `claude` panes whose session UUID could not be resolved (no marker / no cwd)
    /// — left bare, so they start a new chat on restore.
    pub missing_marker: usize,
    /// True if the KDL failed to parse and the raw layout was saved unchanged.
    pub parse_failed: bool,
}

/// Backwards-compatible convenience wrapper: enrich without auto-enter and discard
/// the stats. This is the long-standing pure API the regression suite pins; the
/// plugin itself calls [`enrich_layout`] for the auto-enter flag + stats.
// (Only the test suite calls this now — the wasm binary uses `enrich_layout` — so it
// reads as dead code in the non-test build; kept as the documented pure entry point.)
#[cfg_attr(not(test), allow(dead_code))]
pub fn enrich_claude_panes(kdl: &str, resolve: &SessionResolver<'_>) -> String {
    enrich_layout(kdl, resolve, false).0
}

/// Parse the dumped layout KDL, inject `args "--resume" "<uuid>"` into every
/// restorable `claude` pane whose session UUID can be resolved via `resolve`,
/// and return the re-serialized KDL plus an [`EnrichStats`] summary. On any parse
/// failure the original KDL is returned unchanged (with `parse_failed = true`) so
/// the snapshot is still saved (just without enrichment).
///
/// When `auto_enter` is true, panes that end up with a resume id also have their
/// `start_suspended` child dropped, so `claude --resume …` launches automatically
/// on restore instead of waiting for ENTER. Only panes we recognize as claude AND
/// can pin get auto-launched — every other command pane keeps the safe suspended
/// default, so restore never auto-runs an arbitrary command.
pub fn enrich_layout(
    kdl: &str,
    resolve: &SessionResolver<'_>,
    auto_enter: bool,
) -> (String, EnrichStats) {
    // zellij dumps KDL v1 syntax (it uses the kdl v4 crate), so parse and
    // re-serialize as v1 — a v2 round-trip would produce a layout zellij can't read.
    let mut doc = match KdlDocument::parse_v1(kdl) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!(
                "[zellij-claude-sync] KDL parse failed, saving raw layout: {}",
                e
            );
            return (
                kdl.to_string(),
                EnrichStats {
                    parse_failed: true,
                    ..EnrichStats::default()
                },
            );
        }
    };
    let mut stats = EnrichStats::default();
    enrich_nodes(
        doc.nodes_mut(),
        None,
        false,
        auto_enter,
        resolve,
        &mut stats,
    );
    doc.ensure_v1();
    (doc.to_string(), stats)
}

/// Names whose subtrees are *templates*, not captured session state. Panes inside
/// them describe what to spawn for a brand-new tab, so they must NOT be pinned to
/// a specific (old) session id.
pub(crate) fn is_template_node(name: &str) -> bool {
    matches!(
        name,
        "new_tab_template" | "tab_template" | "swap_tiled_layout" | "swap_floating_layout"
    )
}

/// Recursively walk the layout tree, enriching `claude` panes that hold live state.
/// `inherited_base` is the nearest ancestor `cwd "…"` value, used to resolve
/// relative pane `cwd` properties to absolute paths.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enrich_nodes(
    nodes: &mut [KdlNode],
    inherited_base: Option<String>,
    in_template: bool,
    auto_enter: bool,
    resolve: &SessionResolver<'_>,
    stats: &mut EnrichStats,
) {
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

        if !entering_template && name == "pane" && !neutralize_snap_pane(node) {
            maybe_enrich_pane(node, scope_base.as_deref(), auto_enter, resolve, stats);
        }

        if let Some(children) = node.children_mut().as_mut() {
            enrich_nodes(
                children.nodes_mut(),
                scope_base.clone(),
                entering_template,
                auto_enter,
                resolve,
                stats,
            );
        }
    }
}

/// A pane that was running the `snap` command itself (`zellij pipe … --name save`,
/// optionally wrapped in `timeout`) gets captured verbatim by the dump and would
/// re-run the save on restore — hanging on the never-closed CLI pipe and
/// re-overwriting the snapshot mid-restore. Detect it and strip its `command`/`args`
/// so it restores as a plain shell pane (cwd/size/focus preserved). Returns true if
/// the pane was neutralized.
pub(crate) fn neutralize_snap_pane(node: &mut KdlNode) -> bool {
    let is_wrapper = node
        .entry("command")
        .and_then(|e| e.value().as_string())
        .map(|c| matches!(basename(c), "zellij" | "timeout"))
        .unwrap_or(false);
    if !is_wrapper {
        return false;
    }
    let args: Vec<String> = node
        .children()
        .and_then(|doc| doc.nodes().iter().find(|n| n.name().value() == "args"))
        .map(|n| {
            n.entries()
                .iter()
                .filter_map(|e| e.value().as_string().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let joined = args.join(" ");
    let is_snap = args.iter().any(|a| a == "save")
        && (joined.contains("pipe") || joined.contains("zellij-claude-sync"));
    if !is_snap {
        return false;
    }
    // Drop `command` and every property that is only valid alongside a command
    // (zellij rejects e.g. `start_suspended` on a command-less pane).
    node.entries_mut().retain(|e| {
        !matches!(
            e.name().map(|n| n.value()),
            Some("command") | Some("start_suspended") | Some("close_on_exit")
        )
    });
    if let Some(children) = node.children_mut().as_mut() {
        children.nodes_mut().retain(|n| {
            !matches!(
                n.name().value(),
                "args" | "start_suspended" | "close_on_exit"
            )
        });
    }
    true
}

fn maybe_enrich_pane(
    node: &mut KdlNode,
    base_cwd: Option<&str>,
    auto_enter: bool,
    resolve: &SessionResolver<'_>,
    stats: &mut EnrichStats,
) {
    let is_claude = node
        .entry("command")
        .and_then(|e| e.value().as_string())
        .map(|cmd| basename(cmd) == "claude")
        .unwrap_or(false);
    if !is_claude {
        return;
    }

    // Don't clobber a pane that already carries an explicit --resume / --session-id.
    // It's already resumable, so honor auto-enter for it too.
    if pane_has_session_id(node) {
        stats.already_pinned += 1;
        if auto_enter {
            drop_start_suspended(node);
        }
        return;
    }

    let pane_cwd = node.entry("cwd").and_then(|e| e.value().as_string());
    let full_cwd = match resolve_cwd(pane_cwd, base_cwd) {
        Some(cwd) => cwd,
        None => {
            stats.missing_marker += 1;
            return;
        }
    };

    match resolve(&full_cwd) {
        Some(uuid) => {
            inject_session_id(node, &uuid);
            stats.enriched += 1;
            // Auto-launch on restore ONLY for panes we could actually pin — a bare
            // `claude` with start_suspended dropped would auto-start a *new* chat.
            if auto_enter {
                drop_start_suspended(node);
            }
        }
        None => stats.missing_marker += 1,
    }
}

/// Drop the `start_suspended` child node so the pane's command runs immediately on
/// restore instead of waiting for ENTER. Applied only to panes we pin (see
/// `maybe_enrich_pane`), so restore never auto-runs an unrecognized command.
fn drop_start_suspended(node: &mut KdlNode) {
    if let Some(children) = node.children_mut().as_mut() {
        children
            .nodes_mut()
            .retain(|n| n.name().value() != "start_suspended");
    }
}

/// Resolve a pane's `cwd` property to an absolute path. Absolute values pass
/// through; relative values are joined onto the inherited base cwd; a pane with no
/// `cwd` inherits the base directly.
pub(crate) fn resolve_cwd(pane_cwd: Option<&str>, base_cwd: Option<&str>) -> Option<String> {
    match pane_cwd {
        Some(cwd) if cwd.starts_with('/') => Some(cwd.to_string()),
        Some(cwd) => base_cwd.map(|base| format!("{}/{}", base.trim_end_matches('/'), cwd)),
        None => base_cwd.map(|base| base.to_string()),
    }
}

pub(crate) fn pane_has_session_id(node: &KdlNode) -> bool {
    node.children()
        .map(|doc| {
            doc.nodes().iter().any(|n| {
                n.name().value() == "args"
                    && n.entries().iter().any(|e| {
                        matches!(
                            e.value().as_string(),
                            Some("--resume") | Some("--session-id")
                        )
                    })
            })
        })
        .unwrap_or(false)
}

pub(crate) fn inject_session_id(node: &mut KdlNode, uuid: &str) {
    let children = node.ensure_children();
    if let Some(args) = children
        .nodes_mut()
        .iter_mut()
        .find(|n| n.name().value() == "args")
    {
        // Prepend so --resume leads any user-provided args.
        args.entries_mut()
            .insert(0, KdlEntry::new(uuid.to_string()));
        args.entries_mut().insert(0, KdlEntry::new("--resume"));
    } else {
        let mut args = KdlNode::new("args");
        args.push(KdlEntry::new("--resume"));
        args.push(KdlEntry::new(uuid.to_string()));
        children.nodes_mut().push(args);
    }
}

pub(crate) fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Placeholder UUID used across all fixtures (R-SEC-001: never a real session id)
    // -------------------------------------------------------------------------
    const UUID: &str = "00000000-0000-0000-0000-000000000000";
    const UUID2: &str = "11111111-1111-1111-1111-111111111111";

    // Resolver stubs
    fn resolver_some() -> impl Fn(&str) -> Option<String> {
        |_| Some(UUID.to_string())
    }
    fn resolver_none() -> impl Fn(&str) -> Option<String> {
        |_| None
    }

    // -------------------------------------------------------------------------
    // AC-tests-008: unparseable KDL → raw returned unchanged
    // -------------------------------------------------------------------------
    #[test]
    fn parse_failure_returns_raw_unchanged_ac_tests_008() {
        let bad = "this { is not valid kdl !!!";
        let result = enrich_claude_panes(bad, &resolver_none());
        assert_eq!(
            result, bad,
            "AC-tests-008: parse failure must return raw input"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-009: empty / whitespace-only input does not panic
    // -------------------------------------------------------------------------
    #[test]
    fn empty_input_no_panic_ac_tests_009() {
        // Empty KDL is valid v1 — parses to an empty document, serializes back to "".
        let result = enrich_claude_panes("", &resolver_none());
        assert_eq!(
            result, "",
            "AC-tests-009: empty input must round-trip to empty string"
        );
    }

    #[test]
    fn whitespace_only_input_no_panic_ac_tests_009() {
        // Whitespace-only is valid v1 (empty document with whitespace prefix).
        // ensure_v1() + to_string() returns the original document text unchanged
        // because there are no nodes to re-emit.
        let result = enrich_claude_panes("   \n\t  ", &resolver_none());
        assert_eq!(
            result, "   \n\t  ",
            "AC-tests-009: whitespace-only input must round-trip to the same whitespace"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-007: parity — representative claude-pane layout, resolver returns UUID.
    // Byte-identical comparison against the exact KDL v1 serialization (ADR-004).
    // Expected string derived from the fixture + the injection rule:
    //   inject_session_id prepends args "--resume" UUID as a child node; the KDL v1
    //   serializer escapes '/' in string values (\/), emits the injected args node
    //   without indentation (inserted at index 0 before the formatter runs), and
    //   terminates the args line with a semicolon.
    // -------------------------------------------------------------------------
    const PARITY_INPUT: &str = "layout {\n    cwd \"/home/user\"\n    pane command=\"claude\" {\n        start_suspended true\n    }\n}\n";
    const PARITY_EXPECTED: &str = "layout {\n    cwd \"\\/home\\/user\"\n    pane command=\"claude\" {\n        start_suspended true\nargs \"--resume\" \"00000000-0000-0000-0000-000000000000\";\n    }\n}\n";

    #[test]
    fn parity_claude_pane_enriched_with_resume_ac_tests_007() {
        // Minimal KDL v1 layout with one claude pane; resolver returns UUID.
        // PARITY_EXPECTED is the byte-identical output the KDL v1 serializer produces
        // after injecting args "--resume" UUID (BR-tests-002, AC-tests-007).
        let result = enrich_claude_panes(PARITY_INPUT, &resolver_some());
        assert_eq!(
            result, PARITY_EXPECTED,
            "AC-tests-007: enriched output must be byte-identical to PARITY_EXPECTED"
        );
        // Explicit negative: --session-id must NOT be used (AC-tests-006 / BR-tests-005)
        assert!(
            !result.contains("\"--session-id\""),
            "AC-tests-006: must use --resume not --session-id"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-007 (negative): resolver returns None → pane left bare (no args injected)
    // -------------------------------------------------------------------------
    #[test]
    fn parity_no_marker_pane_left_bare_ac_tests_007_neg() {
        let input = r#"layout {
    cwd "/home/user"
    pane command="claude" {
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_none());
        assert!(
            !result.contains("\"--resume\""),
            "AC-tests-007(neg): no marker means no --resume injected"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-015: idempotent — applying enrichment twice is stable
    // -------------------------------------------------------------------------
    #[test]
    fn idempotent_double_enrichment_ac_tests_015() {
        let input = r#"layout {
    cwd "/home/user"
    pane command="claude" {
        start_suspended true
    }
}
"#;
        let first = enrich_claude_panes(input, &resolver_some());
        let second = enrich_claude_panes(&first, &resolver_some());
        assert_eq!(first, second, "AC-tests-015: enrichment must be idempotent");
    }

    // -------------------------------------------------------------------------
    // AC-tests-010: snap-pane (timeout) has command, args, start_suspended, close_on_exit
    // child nodes ALL stripped — the Jun 29 regression
    // -------------------------------------------------------------------------
    #[test]
    fn snap_pane_timeout_all_children_stripped_ac_tests_010() {
        // Fixture: a pane running `timeout 3 zellij pipe --plugin … --name save -- snap`
        // with start_suspended and close_on_exit child nodes (as dumped by zellij)
        let input = r#"layout {
    pane command="timeout" {
        args "3" "zellij" "pipe" "--plugin" "file:/path/to/zellij-claude-sync.wasm" "--name" "save" "--" "snap"
        start_suspended true
        close_on_exit true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        // The snap pane should have no command, no args, no start_suspended, no close_on_exit
        assert!(
            !result.contains("command=\"timeout\""),
            "AC-tests-010: snap pane command must be stripped"
        );
        assert!(
            !result.contains("start_suspended"),
            "AC-tests-010: start_suspended child node must be stripped"
        );
        assert!(
            !result.contains("close_on_exit"),
            "AC-tests-010: close_on_exit child node must be stripped"
        );
        // No --resume injected into the neutralized snap pane
        assert!(
            !result.contains("\"--resume\""),
            "AC-tests-010: neutralized snap pane must not get --resume"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-011: snap-pane detected via `zellij` basename (not timeout)
    // -------------------------------------------------------------------------
    #[test]
    fn snap_pane_zellij_basename_neutralized_ac_tests_011() {
        let input = r#"layout {
    pane command="zellij" {
        args "pipe" "--plugin" "file:/path/zellij-claude-sync.wasm" "--name" "save" "--" "mysnap"
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        assert!(
            !result.contains("command=\"zellij\""),
            "AC-tests-011: snap pane with zellij basename must be neutralized"
        );
        assert!(
            !result.contains("start_suspended"),
            "AC-tests-011: start_suspended must be stripped from snap pane"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-012: non-snap zellij pane (no save args) is NOT neutralized
    // -------------------------------------------------------------------------
    #[test]
    fn non_snap_zellij_pane_left_intact_ac_tests_012() {
        // command="zellij" but args do NOT contain "save" + pipe
        let input = r#"layout {
    pane command="zellij" {
        args "attach" "mysession"
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        // command should be retained
        assert!(
            result.contains("command=\"zellij\""),
            "AC-tests-012: non-snap zellij pane must keep its command"
        );
        assert!(
            result.contains("start_suspended"),
            "AC-tests-012: non-snap zellij pane must keep start_suspended"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-013: pane with existing --resume is NOT double-injected
    // -------------------------------------------------------------------------
    #[test]
    fn existing_resume_not_double_injected_ac_tests_013() {
        let input = format!(
            r#"layout {{
    cwd "/home/user"
    pane command="claude" {{
        args "--resume" "{uuid}"
        start_suspended true
    }}
}}
"#,
            uuid = UUID
        );
        let result = enrich_claude_panes(&input, &resolver_some());
        // Count occurrences of "--resume" — should be exactly 1
        let count = result.matches("\"--resume\"").count();
        assert_eq!(
            count, 1,
            "AC-tests-013: --resume must appear exactly once, not doubled"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-014: pane with existing --session-id is left pinned (not enriched)
    // -------------------------------------------------------------------------
    #[test]
    fn existing_session_id_left_pinned_ac_tests_014() {
        let input = format!(
            r#"layout {{
    cwd "/home/user"
    pane command="claude" {{
        args "--session-id" "{uuid}"
        start_suspended true
    }}
}}
"#,
            uuid = UUID2
        );
        let result = enrich_claude_panes(&input, &resolver_some());
        // --session-id must still be there, no --resume prepended
        assert!(
            result.contains("\"--session-id\""),
            "AC-tests-014: --session-id must be preserved"
        );
        assert!(
            !result.contains("\"--resume\""),
            "AC-tests-014: no --resume should be injected into an already-pinned pane"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-016: claude pane INSIDE a template subtree is NOT enriched
    // -------------------------------------------------------------------------
    #[test]
    fn claude_pane_in_template_not_enriched_ac_tests_016() {
        let input = r#"layout {
    cwd "/home/user"
    new_tab_template {
        pane command="claude" {
            start_suspended true
        }
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        assert!(
            !result.contains("\"--resume\""),
            "AC-tests-016: claude pane inside new_tab_template must NOT be enriched"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-017: same pane OUTSIDE template IS enriched (contrast)
    // -------------------------------------------------------------------------
    #[test]
    fn claude_pane_outside_template_is_enriched_ac_tests_017() {
        let input = r#"layout {
    cwd "/home/user"
    tab {
        pane command="claude" {
            start_suspended true
        }
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        assert!(
            result.contains("\"--resume\""),
            "AC-tests-017: claude pane outside template must be enriched"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-016 extended: all four template node types are skipped
    // -------------------------------------------------------------------------
    #[test]
    fn all_template_node_types_skipped_ac_tests_016_extended() {
        for template_name in &[
            "new_tab_template",
            "tab_template",
            "swap_tiled_layout",
            "swap_floating_layout",
        ] {
            let input = format!(
                r#"layout {{
    cwd "/home/user"
    {template} {{
        pane command="claude" {{
            start_suspended true
        }}
    }}
}}
"#,
                template = template_name
            );
            let result = enrich_claude_panes(&input, &resolver_some());
            assert!(
                !result.contains("\"--resume\""),
                "AC-tests-016: claude pane inside {} must NOT be enriched",
                template_name
            );
        }
    }

    // -------------------------------------------------------------------------
    // AC-tests-018: relative cwd joined onto base (trailing slash trimmed)
    // -------------------------------------------------------------------------
    #[test]
    fn resolve_cwd_relative_joined_onto_base_ac_tests_018() {
        assert_eq!(
            resolve_cwd(Some("api"), Some("/home/u")),
            Some("/home/u/api".to_string()),
            "AC-tests-018: relative cwd must be joined onto base"
        );
        // trailing slash on base trimmed
        assert_eq!(
            resolve_cwd(Some("api"), Some("/home/u/")),
            Some("/home/u/api".to_string()),
            "AC-tests-018: trailing slash on base must be trimmed"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-019: absolute pane cwd passes through unchanged
    // -------------------------------------------------------------------------
    #[test]
    fn resolve_cwd_absolute_passes_through_ac_tests_019() {
        assert_eq!(
            resolve_cwd(Some("/srv/x"), Some("/home/u")),
            Some("/srv/x".to_string()),
            "AC-tests-019: absolute cwd must pass through unchanged"
        );
        assert_eq!(
            resolve_cwd(Some("/srv/x"), None),
            Some("/srv/x".to_string()),
            "AC-tests-019: absolute cwd passes through with no base"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-020: no pane cwd → inherits base
    // -------------------------------------------------------------------------
    #[test]
    fn resolve_cwd_no_pane_cwd_inherits_base_ac_tests_020() {
        assert_eq!(
            resolve_cwd(None, Some("/home/u")),
            Some("/home/u".to_string()),
            "AC-tests-020: no pane cwd must inherit base"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-021: no cwd and no base → None → pane left bare (no panic)
    // -------------------------------------------------------------------------
    #[test]
    fn resolve_cwd_none_none_returns_none_ac_tests_021() {
        assert_eq!(
            resolve_cwd(None, None),
            None,
            "AC-tests-021: no cwd + no base must return None"
        );
    }

    #[test]
    fn pane_no_cwd_no_base_left_bare_no_panic_ac_tests_021() {
        // No layout-level cwd, no pane-level cwd → pane left bare, no panic
        let input = r#"layout {
    pane command="claude" {
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        // No cwd at all → can't resolve → pane left without --resume
        assert!(
            !result.contains("\"--resume\""),
            "AC-tests-021: claude pane with no resolvable cwd must be left bare"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-022: inject_session_id prepends --resume preserving existing args
    // -------------------------------------------------------------------------
    #[test]
    fn inject_prepends_resume_preserving_existing_args_ac_tests_022() {
        let input = r#"layout {
    cwd "/home/user"
    pane command="claude" {
        args "my-prompt"
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        // --resume and UUID must appear before "my-prompt"
        let resume_pos = result
            .find("\"--resume\"")
            .expect("--resume must be present");
        let prompt_pos = result
            .find("\"my-prompt\"")
            .expect("my-prompt must be present");
        assert!(
            resume_pos < prompt_pos,
            "AC-tests-022: --resume must be prepended before existing args"
        );
        assert!(
            result.contains(&format!("\"{}\"", UUID)),
            "AC-tests-022: UUID must be present"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-023: inject_session_id creates args block when none exists
    // -------------------------------------------------------------------------
    #[test]
    fn inject_creates_args_block_when_none_ac_tests_023() {
        let input = r#"layout {
    cwd "/home/user"
    pane command="claude" {
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        assert!(
            result.contains("\"--resume\""),
            "AC-tests-023: args block must be created with --resume when none existed"
        );
        assert!(
            result.contains(&format!("\"{}\"", UUID)),
            "AC-tests-023: args block must contain the UUID"
        );
    }

    // -------------------------------------------------------------------------
    // AC-tests-024: basename on /usr/bin/claude → claude (matches); vim → not enriched
    // -------------------------------------------------------------------------
    #[test]
    fn basename_full_path_extracts_name_ac_tests_024() {
        assert_eq!(
            basename("/usr/bin/claude"),
            "claude",
            "AC-tests-024: basename must extract 'claude' from full path"
        );
        assert_eq!(
            basename("claude"),
            "claude",
            "AC-tests-024: bare name passes through"
        );
        assert_eq!(
            basename("/usr/bin/vim"),
            "vim",
            "AC-tests-024: basename extracts vim correctly"
        );
    }

    #[test]
    fn path_qualified_claude_is_enriched_ac_tests_024() {
        let input = r#"layout {
    cwd "/home/user"
    pane command="/usr/bin/claude" {
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        assert!(
            result.contains("\"--resume\""),
            "AC-tests-024: /usr/bin/claude must be enriched (basename matches)"
        );
    }

    #[test]
    fn vim_pane_not_enriched_ac_tests_024() {
        let input = r#"layout {
    cwd "/home/user"
    pane command="vim" {
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver_some());
        assert!(
            !result.contains("\"--resume\""),
            "AC-tests-024: vim pane must NOT be enriched"
        );
    }

    // -------------------------------------------------------------------------
    // is_template_node unit tests (AC-tests-016/017)
    // -------------------------------------------------------------------------
    #[test]
    fn is_template_node_recognizes_all_four_ac_tests_016() {
        assert!(
            is_template_node("new_tab_template"),
            "AC-tests-016: new_tab_template is a template"
        );
        assert!(
            is_template_node("tab_template"),
            "AC-tests-016: tab_template is a template"
        );
        assert!(
            is_template_node("swap_tiled_layout"),
            "AC-tests-016: swap_tiled_layout is a template"
        );
        assert!(
            is_template_node("swap_floating_layout"),
            "AC-tests-016: swap_floating_layout is a template"
        );
        assert!(
            !is_template_node("tab"),
            "AC-tests-017: tab is NOT a template"
        );
        assert!(
            !is_template_node("pane"),
            "AC-tests-017: pane is NOT a template"
        );
        assert!(
            !is_template_node("layout"),
            "AC-tests-017: layout is NOT a template"
        );
    }

    // -------------------------------------------------------------------------
    // pane_has_session_id unit tests (AC-tests-013/014)
    // -------------------------------------------------------------------------
    #[test]
    fn pane_has_session_id_detects_resume_ac_tests_013() {
        // Build a node with --resume in args
        let kdl = format!(
            r#"pane command="claude" {{
    args "--resume" "{uuid}"
}}"#,
            uuid = UUID
        );
        let doc = KdlDocument::parse_v1(&kdl).unwrap();
        let node = &doc.nodes()[0];
        assert!(
            pane_has_session_id(node),
            "AC-tests-013: --resume must be detected as pinned"
        );
    }

    #[test]
    fn pane_has_session_id_detects_session_id_ac_tests_014() {
        let kdl = format!(
            r#"pane command="claude" {{
    args "--session-id" "{uuid}"
}}"#,
            uuid = UUID
        );
        let doc = KdlDocument::parse_v1(&kdl).unwrap();
        let node = &doc.nodes()[0];
        assert!(
            pane_has_session_id(node),
            "AC-tests-014: --session-id must be detected as pinned"
        );
    }

    #[test]
    fn pane_has_session_id_returns_false_for_plain_args_ac_tests_013() {
        let kdl = r#"pane command="claude" {
    args "my-prompt"
}"#;
        let doc = KdlDocument::parse_v1(kdl).unwrap();
        let node = &doc.nodes()[0];
        assert!(
            !pane_has_session_id(node),
            "plain args must not be detected as pinned"
        );
    }

    // -------------------------------------------------------------------------
    // neutralize_snap_pane unit tests (AC-tests-010/011/012)
    // -------------------------------------------------------------------------
    #[test]
    fn neutralize_snap_pane_strips_all_child_nodes_ac_tests_010() {
        let kdl = r#"pane command="timeout" {
    args "3" "zellij" "pipe" "--name" "save" "--" "snap"
    start_suspended true
    close_on_exit true
}"#;
        let mut doc = KdlDocument::parse_v1(kdl).unwrap();
        let node = &mut doc.nodes_mut()[0];
        let neutralized = neutralize_snap_pane(node);
        assert!(neutralized, "AC-tests-010: snap pane must be neutralized");
        // command entry must be gone
        assert!(
            node.entry("command").is_none(),
            "AC-tests-010: command entry must be stripped"
        );
        // no start_suspended or close_on_exit or args children
        if let Some(children) = node.children() {
            for child in children.nodes() {
                let cn = child.name().value();
                assert!(
                    !matches!(cn, "start_suspended" | "close_on_exit" | "args"),
                    "AC-tests-010: child node '{}' must be stripped",
                    cn
                );
            }
        }
    }

    #[test]
    fn neutralize_snap_pane_returns_false_for_non_snap_ac_tests_012() {
        // A zellij pane but without save args
        let kdl = r#"pane command="zellij" {
    args "attach" "mysession"
    start_suspended true
}"#;
        let mut doc = KdlDocument::parse_v1(kdl).unwrap();
        let node = &mut doc.nodes_mut()[0];
        let neutralized = neutralize_snap_pane(node);
        assert!(
            !neutralized,
            "AC-tests-012: non-snap pane must NOT be neutralized"
        );
        // command must still be there
        assert!(
            node.entry("command").is_some(),
            "AC-tests-012: command must be preserved"
        );
    }

    #[test]
    fn neutralize_snap_pane_returns_false_for_claude_pane_ac_tests_012() {
        let kdl = format!(
            r#"pane command="claude" {{
    args "--resume" "{uuid}"
    start_suspended true
}}"#,
            uuid = UUID
        );
        let mut doc = KdlDocument::parse_v1(&kdl).unwrap();
        let node = &mut doc.nodes_mut()[0];
        let neutralized = neutralize_snap_pane(node);
        assert!(
            !neutralized,
            "AC-tests-012: claude pane must NOT be neutralized"
        );
    }

    // -------------------------------------------------------------------------
    // Regression: cwd resolution in full enrichment pipeline (integration of
    // resolve_cwd + inject through enrich_claude_panes) — covers the relative-cwd
    // join case end-to-end (BR-tests-008: relative-vs-absolute cwd resolution)
    // -------------------------------------------------------------------------
    #[test]
    fn relative_cwd_resolved_and_enriched_br_tests_008() {
        // Resolver tracks what cwd it was called with so we can assert the join
        use std::cell::RefCell;
        let called_with: RefCell<Vec<String>> = RefCell::new(vec![]);
        let resolver = |cwd: &str| -> Option<String> {
            called_with.borrow_mut().push(cwd.to_string());
            Some(UUID.to_string())
        };

        let input = r#"layout {
    cwd "/home/user"
    pane command="claude" cwd="projects" {
        start_suspended true
    }
}
"#;
        let result = enrich_claude_panes(input, &resolver);
        let calls = called_with.borrow();
        assert_eq!(
            calls.as_slice(),
            ["/home/user/projects"],
            "BR-tests-008: resolver must be called with the joined absolute cwd"
        );
        assert!(
            result.contains("\"--resume\""),
            "BR-tests-008: enrichment must inject --resume"
        );
    }

    #[test]
    fn absolute_cwd_passthrough_enriched_br_tests_008() {
        use std::cell::RefCell;
        let called_with: RefCell<Vec<String>> = RefCell::new(vec![]);
        let resolver = |cwd: &str| -> Option<String> {
            called_with.borrow_mut().push(cwd.to_string());
            Some(UUID.to_string())
        };

        let input = r#"layout {
    cwd "/home/user"
    pane command="claude" cwd="/srv/api" {
        start_suspended true
    }
}
"#;
        let _result = enrich_claude_panes(input, &resolver);
        let calls = called_with.borrow();
        assert_eq!(
            calls.as_slice(),
            ["/srv/api"],
            "BR-tests-008: absolute cwd must pass through to resolver unchanged"
        );
    }

    // =========================================================================
    // enrich_layout: auto-enter flag + EnrichStats (v0.2 features)
    // =========================================================================

    const CLAUDE_LAYOUT: &str = "layout {\n    cwd \"/home/user\"\n    pane command=\"claude\" {\n        start_suspended true\n    }\n}\n";

    #[test]
    fn auto_enter_off_preserves_start_suspended() {
        // Default/back-compat behavior: with auto_enter=false the enriched claude
        // pane keeps `start_suspended`, so it waits for ENTER on restore.
        let (result, _stats) = enrich_layout(CLAUDE_LAYOUT, &resolver_some(), false);
        assert!(
            result.contains("--resume"),
            "auto_enter=false must still inject --resume"
        );
        assert!(
            result.contains("start_suspended"),
            "auto_enter=false must preserve start_suspended"
        );
    }

    #[test]
    fn auto_enter_on_drops_start_suspended_on_enriched_pane() {
        // With auto_enter=true a pane we pin loses `start_suspended`, so
        // `claude --resume …` launches automatically on restore.
        let (result, stats) = enrich_layout(CLAUDE_LAYOUT, &resolver_some(), true);
        assert!(
            result.contains("--resume"),
            "auto_enter=true must inject --resume"
        );
        assert!(
            !result.contains("start_suspended"),
            "auto_enter=true must drop start_suspended on the enriched pane"
        );
        assert_eq!(stats.enriched, 1, "one claude pane enriched");
    }

    #[test]
    fn auto_enter_on_leaves_unresolved_pane_suspended() {
        // Safety guarantee: a claude pane we CAN'T pin (no marker) keeps
        // start_suspended even with auto_enter=true — otherwise a bare `claude`
        // would auto-start a NEW chat on restore.
        let (result, stats) = enrich_layout(CLAUDE_LAYOUT, &resolver_none(), true);
        assert!(!result.contains("--resume"), "no marker → no --resume");
        assert!(
            result.contains("start_suspended"),
            "auto_enter must NOT drop start_suspended on an unpinned pane"
        );
        assert_eq!(stats.missing_marker, 1, "one claude pane had no marker");
        assert_eq!(stats.enriched, 0);
    }

    #[test]
    fn auto_enter_does_not_touch_non_claude_command_panes() {
        // A non-claude command pane must keep start_suspended regardless of
        // auto_enter — we never auto-run arbitrary commands on restore.
        let input = "layout {\n    cwd \"/home/user\"\n    pane command=\"vim\" {\n        start_suspended true\n    }\n    pane command=\"claude\" {\n        start_suspended true\n    }\n}\n";
        let (result, stats) = enrich_layout(input, &resolver_some(), true);
        assert!(
            result.contains("command=\"vim\""),
            "vim pane must still be present"
        );
        // The vim pane's start_suspended must survive; only the claude one is dropped.
        let suspended_count = result.matches("start_suspended").count();
        assert_eq!(
            suspended_count, 1,
            "only the vim pane keeps start_suspended (claude's is dropped)"
        );
        assert_eq!(stats.enriched, 1, "only the claude pane is enriched");
    }

    #[test]
    fn stats_count_already_pinned() {
        // A pane already carrying --resume is counted as already_pinned, not enriched.
        let input = "layout {\n    cwd \"/home/user\"\n    pane command=\"claude\" {\n        args \"--resume\" \"deadbeef\"\n        start_suspended true\n    }\n}\n";
        let (_result, stats) = enrich_layout(input, &resolver_some(), false);
        assert_eq!(stats.already_pinned, 1);
        assert_eq!(stats.enriched, 0);
    }

    #[test]
    fn stats_parse_failed_flag_set_on_bad_kdl() {
        let (result, stats) = enrich_layout("not { valid kdl !!!", &resolver_some(), true);
        assert!(stats.parse_failed, "parse_failed must be set on bad KDL");
        assert_eq!(
            result, "not { valid kdl !!!",
            "raw input returned unchanged"
        );
        assert_eq!(stats.enriched, 0);
    }

    #[test]
    fn auto_enter_on_pins_and_launches_already_pinned_pane() {
        // An already-pinned pane also gets start_suspended dropped under auto_enter,
        // so re-snapping a resumable layout keeps it auto-launching.
        let input = "layout {\n    cwd \"/home/user\"\n    pane command=\"claude\" {\n        args \"--resume\" \"deadbeef\"\n        start_suspended true\n    }\n}\n";
        let (result, stats) = enrich_layout(input, &resolver_some(), true);
        assert!(
            !result.contains("start_suspended"),
            "auto_enter must drop start_suspended on an already-pinned pane too"
        );
        assert_eq!(stats.already_pinned, 1);
    }
}
