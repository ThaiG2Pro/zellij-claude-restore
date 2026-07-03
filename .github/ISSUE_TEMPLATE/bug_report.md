---
name: Bug report
about: Something didn't restore / resume the way you expected
title: ""
labels: bug
---

**What happened vs. what you expected**
<!-- e.g. "claude pane started a new chat instead of resuming" -->

**Environment**
- Zellij version: <!-- zellij --version -->
- OS:
- Shell: <!-- fish / bash / zsh -->
- Installed from: <!-- source build / release .wasm -->

**Marker present?**
<!-- ls /tmp/zellij-$(id -u)/claude-sessions/  — is there a marker for the pane's cwd? -->

**How you launched claude**
<!-- bare `claude`? with a prompt? multiple claude panes in the same directory? -->

**Snapshot KDL (optional but very helpful)**
<!-- ~/.config/zellij/layouts/<name>.kdl — scrub any session UUIDs you don't want to share -->

```kdl

```

**snap output**
<!-- the "✓ saved snapshot / N claude pane(s) will resume …" lines -->
