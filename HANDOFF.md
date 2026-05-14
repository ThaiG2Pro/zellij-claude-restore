# Zellij × Claude Code Session Sync — Tài liệu bàn giao

**Ngày**: 2026-05-14  
**Tác giả**: Research & Architecture phase  
**Đối tượng**: BA, Tech Lead  
**Trạng thái**: Sẵn sàng để lên plan và implement  

---

## 1. Tóm tắt (Executive Summary)

Developer dùng Zellij (terminal multiplexer) với nhiều pane Claude Code mở đồng thời. Sau khi reboot hoặc crash, Zellij có khả năng resurrection (khôi phục layout), nhưng **không thể tự động resume đúng session chat Claude** vì Claude Code không expose current session ID ra process argv. Giải pháp là viết một **Zellij plugin** cho phép user **chủ động lưu snapshot workspace** bao gồm layout + mapping pane → Claude session ID. Restore là explicit: `zellij --layout <snapshot-name>`.

---

## 2. Bối cảnh & Vấn đề

### 2.1 Workflow của developer

```
┌─────────────────────────────────────────────────┐
│  Zellij session                                  │
│                                                  │
│  Tab "api"           Tab "infra"                 │
│  ┌──────────────┐    ┌──────────────┐            │
│  │ claude chat  │    │ claude chat  │            │
│  │ (session A)  │    │ (session B)  │            │
│  ├──────────────┤    ├──────────────┤            │
│  │   vim        │    │   terminal   │            │
│  └──────────────┘    └──────────────┘            │
└─────────────────────────────────────────────────┘
```

- Nhiều pane claude mở song song, mỗi pane có context chat riêng
- Developer muốn sau reboot, restore được **đúng layout + đúng session chat** từng pane
- Đây không phải tính năng có sẵn — cần build thêm

### 2.2 Vấn đề cụ thể

Sau reboot:
1. Zellij resurrection restore **layout + cwd** ✅
2. Mỗi pane claude được spawn lại với command `claude` ✅
3. Nhưng spawn `claude` không có args → **tạo session chat MỚI** ❌
4. Session cũ vẫn còn trên disk, nhưng không được load tự động ❌

---

## 3. Research Findings — Nội bộ của hai hệ thống

### 3.1 Cách Zellij lưu session (đọc source `~/src/zellij/`)

**Cơ chế snapshot tự động:**
- Background job chạy ticker **60 giây/lần**, gửi `ScreenInstruction::SerializeLayoutForResurrection`
- Screen thread serialize toàn bộ state thành **KDL format** (KDL Document Language)
- File ghi vào: `~/.cache/zellij/contract_version_1/session_info/<session-name>/session-layout.kdl`
- Pane contents (scrollback) ghi riêng ra external files, KDL chứa pointer

**CWD được lấy thế nào:**
- Background job chạy ticker **1 giây/lần**, gửi `PtyInstruction::UpdateAndReportCwds`
- Pty thread gọi `sysinfo` crate → đọc `/proc/<pid>/cwd` của từng shell process
- CWD được ghi vào KDL: `cwd "/home/user/project"`

**Command được lấy thế nào:**
- `sysinfo` cũng gọi `get_cwds(pids)` trả về `HashMap<u32, Vec<String>>` (argv từ `/proc/<pid>/cmdline`)
- KDL ghi: `command="claude" args="--session-id" "abc-123"` (nếu args có)
- Nguồn: `zellij-server/src/background_jobs.rs:154-182`, `pty.rs:1956-1985`

**Resurrection:**
- `find_resurrectable_sessions` scan `session_info/` folder, tìm folder có tồn tại nhưng socket đã biến mất → đó là session "chết"
- Khi user chọn resurrect: Zellij đọc KDL, **replay lại command** từng pane trong đúng cwd

**Write optimization:**
- `file_content_changed(path, new_bytes)` so sánh trước khi ghi → nếu nội dung không đổi thì skip → pane đang im = 0 I/O

### 3.2 Cách Claude Code lưu session (đọc `~/.claude/`)

**Storage path:**
```
~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl
```
- `<encoded-cwd>`: `/home/user/project` → `-home-user-project` (slashes thành dashes)
- Mỗi conversation = 1 UUID = 1 file JSONL

**Format — Append-only event log:**
```json
{"type": "user",    "uuid": "...", "parentUuid": "...", "message": {...}, "timestamp": "..."}
{"type": "message", "uuid": "...", "parentUuid": "...", "message": {...}, "timestamp": "..."}
```
- Mỗi dòng = 1 event (user message, assistant message, tool call, hook result, file snapshot, ...)
- `parentUuid` tạo thành linked list → hỗ trợ conversation branching
- Crash-safe: append-only, mất tối đa event đang ghi dở

**Resume hoạt động thế nào:**
- `claude -r <uuid>`: đọc toàn bộ JSONL, reconstruct conversation → gửi lên model → model có đầy đủ context
- `claude -c`: load file có timestamp mới nhất trong project folder

**Lệnh CLI liên quan:**
```
claude -r <session-uuid>        # resume session cụ thể
claude -c                       # resume session mới nhất của cwd
claude --session-id <uuid>      # dùng UUID này cho session mới (hoặc existing)
claude --fork-session           # resume nhưng tạo session ID mới (branch)
```

---

## 4. Root Cause Analysis — Tại sao không tự động được

### 4.1 Vấn đề argv immutability

```
User gõ: claude          → process spawn với argv = ["claude"]
User /resume → pick uuid-X  → claude đổi context nội bộ
              NHƯNG argv vẫn = ["claude"]  ← kernel không cho đổi
              
Zellij đọc /proc/<pid>/cmdline → thấy "claude" (không args)
Zellij serialize KDL → command="claude"
Resurrect → spawn "claude" → SESSION MỚI
```

**UNIX process model**: argv được set 1 lần lúc `execve()`. Process có thể overwrite vùng nhớ argv (kỹ thuật `setproctitle` mà postgres/sshd dùng), nhưng Claude Code (closed-source) chưa làm điều này.

### 4.2 Các hướng fix không khả thi

| Hướng | Tại sao không được |
|---|---|
| PR vào Claude Code | Closed source — không access |
| CRIU (checkpoint/restore process) | Yêu cầu root, không portable (macOS), PTY là chicken-egg problem |
| Wrap claude để giữ PID persistent | exec() thay thế process — không có cách "wrap" mà giữ argv riêng |
| Claude hook gọi setproctitle | Hook chạy trong subprocess, không thể sửa argv của process cha |

### 4.3 Vấn đề /resume UI

Kể cả khi launch `claude --session-id <uuid-A>` rồi user dùng `/resume` để pick `uuid-B`:
- argv vẫn là `--session-id uuid-A`
- Zellij resurrect → load uuid-A, không phải uuid-B
- Không có cách nào detect từ ngoài mà session nào đang active inside claude

---

## 5. Các Giải Pháp Đã Xem Xét

### 5.1 Giải pháp 1 — Wrapper script `cl` (alias)

**Ý tưởng**: Thay `claude` bằng `cl` wrapper, tự generate UUID khi launch:
```bash
cl  →  claude --session-id $(uuidgen)
```
argv chứa UUID → Zellij capture được.

**Vấn đề không giải quyết được**: User dùng `/resume` UI → argv vẫn là UUID-A, không phải session đã pick.

**Kết luận**: Workaround đủ cho 80% case (không dùng /resume), nhưng không giải quyết gốc rễ.

### 5.2 Giải pháp 2 — Plugin race-write (5s tick)

**Ý tưởng**: Plugin ghi đè `session-layout.kdl` mỗi 5s với session IDs đúng.

**Vấn đề**: Race condition với Zellij's 60s tick. Zellij sẽ ghi đè lại với version sai. Không atomic. Reliability thấp.

**Kết luận**: Reject — không production-worthy.

### 5.3 Giải pháp 3 — OverrideLayout khi resurrect

**Ý tưởng**: Plugin load sau resurrect, gọi `OverrideLayout` với layout đúng (từ state file plugin tự maintain).

**Vấn đề**: Panes cũ bị kill và respawn → visual disruption. Semantics của `OverrideLayout` chưa được verify là có thực sự respawn pane với command mới không.

**Kết luận**: Possible nhưng UX không tốt, cần verify thêm.

### 5.4 Giải pháp 4 — Plugin full takeover snapshot

**Ý tưởng**: Disable Zellij built-in snapshot, plugin hoàn toàn chịu trách nhiệm ghi `session-layout.kdl`.

**Vấn đề**: Plugin crash = mất toàn bộ resurrection. Blast radius cao.

**Kết luận**: Reject cho production personal use.

### 5.5 ✅ Giải pháp Chọn — Explicit Named Snapshots via SaveLayout API

**Ý tưởng**: User chủ động trigger save. Plugin dùng API `SaveLayout` để ghi vào layout library. Restore là `zellij --layout <name>`.

**Lý do chọn**:
- Dùng API chính thức của Zellij → ổn định
- Không race với bất kỳ background job nào
- Named snapshots = user control rõ ràng
- Blast radius thấp nếu plugin lỗi
- Có thể extend (nhiều snapshot, version, per-project)
- Workflow quen thuộc: `:mksession` vim, `git stash save <name>`

---

## 6. Kiến Trúc Giải Pháp

### 6.1 Tổng quan

```
┌─────────────────────────────────────────────────────────────┐
│  Zellij session đang chạy                                   │
│                                                             │
│  Pane 1: claude --session-id UUID-A (cwd: ~/api)           │
│  Pane 2: claude --session-id UUID-B (cwd: ~/frontend)      │
│  Pane 3: vim (cwd: ~/api)                                   │
│                                                             │
│  Plugin: zellij-claude-sync (chạy background, invisible)   │
└────────────────────────┬────────────────────────────────────┘
                         │
              User trigger: snap work-friday
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│  Plugin flow                                                │
│                                                             │
│  1. DumpSessionLayout()  → KDL string                      │
│  2. GetPanePid(pane_1)   → PID 12345                       │
│     GetPanePid(pane_2)   → PID 12346                       │
│  3. Resolve UUID per pane:                                  │
│     - Read /tmp/claude-pane-12345.session → UUID-A         │
│     - Read /tmp/claude-pane-12346.session → UUID-B         │
│  4. Enrich KDL:                                            │
│     command="claude" args=["--session-id", "UUID-A"]       │
│  5. SaveLayout("work-friday", enriched_kdl, overwrite=true)│
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
       ~/.config/zellij/layouts/work-friday.kdl
       
══════════════════ Sau reboot ══════════════════

       zellij --layout work-friday
                         │
                         ▼
       Pane 1: claude --session-id UUID-A → resume session A ✅
       Pane 2: claude --session-id UUID-B → resume session B ✅
       Pane 3: vim (cwd ~/api) ✅
```

### 6.2 Components

#### Component 1: Zellij Plugin (`zellij-claude-sync`)

**Language**: Rust → compile sang WASM (`wasm32-wasip1`)

**Dependencies**:
- `zellij-tile` — plugin SDK (event, commands, render)
- `kdl` crate — parse/modify KDL string
- std filesystem (qua plugin FullHdAccess permission)

**Plugin Permissions cần request**:
```
ReadApplicationState   — DumpSessionLayout, GetPaneInfo, GetPanePid
ChangeApplicationState — SaveLayout
FullHdAccess           — đọc /tmp/claude-pane-*.session
```

**Event subscriptions**:
```
CustomMessage          — nhận response DumpSessionLayout
PermissionRequestResult — handle permission grant
```

**Triggers**:
```
pipe()  — nhận command từ CLI: zellij pipe --name save -- <snapshot-name>
```

**State**:
```rust
struct State {
    pending_snapshot_name: Option<String>,  // đang chờ DumpSessionLayout response
}
```

**Flow trong plugin**:
```
pipe(name="save", payload="work-friday")
  → self.pending_snapshot_name = Some("work-friday")
  → dump_session_layout()        // async, response về sau

Event::CustomMessage("session_layout", kdl_string)
  → resolve_session_ids(&kdl_string)  // đọc /tmp markers + fallback heuristic
  → enrich_kdl(kdl_string, session_ids)
  → save_layout("work-friday", enriched, true)
  → self.pending_snapshot_name = None
```

#### Component 2: Claude Code Hook (marker file)

**Setup 1 lần** trong `~/.claude/settings.json`:
```json
{
  "hooks": {
    "SessionStart": [{
      "hooks": [{
        "type": "command",
        "command": "mkdir -p /tmp/claude-sessions && echo $CLAUDE_SESSION_ID > /tmp/claude-sessions/pane-$PPID.session"
      }]
    }]
  }
}
```

**Mục đích**: Mỗi khi claude load/switch session (kể cả qua /resume), ghi UUID ra file keyed theo PID.

**QUAN TRỌNG — Chưa verify**: Hook `SessionStart` có fire khi user dùng `/resume` UI picker không. Đây là **assumption cần verify sớm** (xem mục 9 — Open Questions).

**Fallback nếu hook không work**: Heuristic — scan `~/.claude/projects/<encoded-cwd>/` lấy file `.jsonl` có `mtime` mới nhất. Đúng cho 90% case, sai khi 2 pane cùng cwd đang active cùng lúc.

#### Component 3: CLI trigger script

```fish
# ~/.config/fish/functions/snap.fish
function snap --description 'Save Zellij+Claude workspace snapshot'
    if test (count $argv) -eq 0
        echo "Usage: snap <name>"
        return 1
    end
    zellij pipe --plugin file:~/.config/zellij/plugins/zellij-claude-sync.wasm \
                --name save \
                -- $argv[1]
    echo "Saved snapshot: $argv[1]"
end

function snap-list --description 'List saved snapshots'
    ls ~/.config/zellij/layouts/ | grep -v "^default" | sed 's/.kdl$//'
end

function snap-load --description 'Load snapshot in new tab'
    zellij action new-tab --layout $argv[1]
end
```

**Usage**:
```bash
snap work-friday          # save snapshot
snap-list                 # list snapshots
snap-load work-friday     # restore trong Zellij đang chạy (new tab)
zellij --layout work-friday  # restore khi mở Zellij mới
```

#### Component 4: Plugin auto-start (optional)

Để plugin luôn chạy trong mỗi session (cần cho trigger hoạt động):
```kdl
// ~/.config/zellij/layouts/default.kdl
layout {
    tab_template name="main" {
        children
        pane size=0 borderless=true {
            plugin location="file:~/.config/zellij/plugins/zellij-claude-sync.wasm" {
                hide_pane true
            }
        }
    }
    // ... tabs thực tế của user
}
```

### 6.3 KDL Enrichment — Input/Output

**Input (từ DumpSessionLayout)**:
```kdl
layout {
    tab name="api" cwd="/home/user/api" {
        pane command="claude" cwd="/home/user/api"
        pane command="vim"
    }
    tab name="frontend" cwd="/home/user/frontend" {
        pane command="claude" cwd="/home/user/frontend"
    }
}
```

**Output (sau enrich)**:
```kdl
layout {
    tab name="api" cwd="/home/user/api" {
        pane command="claude" cwd="/home/user/api" {
            args "--session-id" "3939d9e8-a9f6-48b6-90c6-f7abf7a7a06f"
        }
        pane command="vim"
    }
    tab name="frontend" cwd="/home/user/frontend" {
        pane command="claude" cwd="/home/user/frontend" {
            args "--session-id" "0ab8e748-3ddf-4f6c-926f-984e07c2d501"
        }
    }
}
```

### 6.4 Session UUID Resolution Logic

```
Input: pane với command="claude", pane_id=X
Output: Option<String> (UUID)

1. GetPanePid(pane_id) → pid
2. Read /tmp/claude-sessions/pane-<pid>.session
   → Success: return UUID (tin cậy nhất)
   
3. Fallback: GetPaneCwd(pane_id) → cwd
   → encode cwd: replace "/" with "-"
   → scan ~/.claude/projects/<encoded-cwd>/*.jsonl
   → sort by mtime DESC, take first
   → return UUID from filename (tin cậy nếu 1 pane per cwd)
   
4. Fallback cuối: return None
   → Plugin có thể:
     a. Skip pane này (ghi command="claude" không args)
     b. Hiển thị warning (nếu plugin có UI)
     c. Dùng interactive mode: show list sessions cho user pick
```

---

## 7. Decisions Made

| # | Decision | Rationale | Alternatives Rejected |
|---|---|---|---|
| D1 | Explicit user-triggered save (không auto) | Loại bỏ race condition; user biết chính xác khi nào muốn snapshot; workflow quen (`:mksession`, `git stash save`) | Auto 5s tick — race với Zellij; quá nhiều I/O |
| D2 | Dùng `SaveLayout` API (không ghi thẳng file) | API chính thức, ổn định hơn qua Zellij version bumps | Ghi trực tiếp `session-layout.kdl` — không official, có thể conflict |
| D3 | Snapshot riêng (layout library) vs resurrection cache | Tách biệt: Zellij auto-resurrect vẫn là fallback; explicit snapshot là layer trên | Override resurrection cache — blast radius cao |
| D4 | Claude Code hook + /tmp marker file cho UUID detection | Chính xác nhất khi hook fire; non-invasive với claude | Parse claude binary — không thể (closed source); env var — không có |
| D5 | Heuristic newest-jsonl làm fallback | Đủ tốt cho 90% case (1 claude per cwd); không yêu cầu setup thêm | Không có fallback — UX tệ khi hook fail |
| D6 | Rust + WASM plugin (zellij-tile) | Đây là cách duy nhất Zellij plugin hoạt động; type-safe; build artifact portable | Không có lựa chọn khác cho Zellij plugin |
| D7 | CLI pipe trigger (không keybinding mặc định) | Linh hoạt hơn; user có thể tự bind; không conflict keybinding hiện có | Baked-in keybinding — có thể conflict setup user |
| D8 | Không disable Zellij auto-resurrection | Giữ fallback layer (dù layout resurrect args sai, vẫn restore layout + cwd) | Disable built-in — mất fallback, rủi ro cao |

---

## 8. Risks & Mitigations

### Risk 1 — CRITICAL: `SessionStart` hook không fire khi /resume UI
**Khả năng**: Trung (chưa verify)  
**Tác hại**: Plugin dùng heuristic fallback → sai khi 2 pane cùng cwd  
**Mitigation**:  
- Verify sớm nhất — là go/no-go cho accuracy của giải pháp  
- Nếu hook không fire: dùng heuristic + UI interactive confirm  
- Nếu hook fire: giải pháp 100% chính xác  

### Risk 2 — HIGH: `SaveLayout` output format không compatible với restore
**Khả năng**: Thấp (cùng KDL engine Zellij)  
**Tác hại**: Restore fail hoàn toàn  
**Mitigation**: Test sớm với "hello world" plugin — gọi `SaveLayout` với KDL đơn giản, verify file ghi ra, `zellij --layout <name>` thành công  

### Risk 3 — MEDIUM: 2 pane claude cùng cwd → UUID sai
**Khả năng**: Trung (phụ thuộc workflow user)  
**Tác hại**: 1 trong 2 pane resume sai session  
**Mitigation**: Interactive confirm trong plugin UI; hoặc convention "1 cwd 1 claude pane"  

### Risk 4 — MEDIUM: Zellij plugin API breaking change
**Khả năng**: Trung (Zellij < v1.0, đang active development)  
**Tác hại**: Plugin không build được sau Zellij upgrade  
**Mitigation**: Pin `zellij-tile` version trong `Cargo.toml`; test trước khi upgrade Zellij  

### Risk 5 — LOW: Claude `--session-id` semantics thay đổi
**Khả năng**: Thấp  
**Tác hại**: Restore không resume session cũ mà tạo session mới  
**Mitigation**: Monitor Claude Code changelog; abstract qua wrapper nếu cần  

### Risk 6 — LOW: Plugin không được grant `FullHdAccess`
**Khả năng**: Thấp (user grant khi load plugin)  
**Tác hại**: Không đọc được `/tmp/claude-sessions/` marker files; fallback heuristic  
**Mitigation**: Document permission requirement; graceful fallback to heuristic  

### Risk 7 — UX: User quên save → reboot → mất session
**Khả năng**: CAO (human factor)  
**Tác hại**: Mất mapping session, claude mở session mới  
**Mitigation**: Optional cron auto-save mỗi 30 phút với name `auto-<timestamp>`; hiện thị thời gian snapshot cuối trong plugin  

---

## 9. Open Questions (cần resolve trước khi implement)

| # | Câu hỏi | Impact | Cách verify |
|---|---|---|---|
| Q1 | `SessionStart` hook có fire khi `/resume` UI picker không? | **CRITICAL** — quyết định accuracy của UUID detection | Tạo hook test: `echo "session=$CLAUDE_SESSION_ID pid=$PPID" >> /tmp/claude-hook.log`, dùng `/resume` UI, check log |
| Q2 | `SaveLayout` KDL output có restore được đúng `command + args` không? | **CRITICAL** — feasibility của toàn bộ approach | Viết plugin "hello world": SaveLayout với pane có `command + args`, verify file, chạy `zellij --layout` |
| Q3 | `OverrideLayout` có kill-and-respawn pane với command mới không? | Medium — cần nếu muốn Cách 2 làm alternative | Đọc source `zellij_exports.rs:override_layout()` |
| Q4 | Plugin `pipe()` callback có hoạt động khi plugin chạy background (hidden pane) không? | High — trigger mechanism chính | Test với plugin minimal: background pane, trigger pipe từ CLI |
| Q5 | Env var `CLAUDE_SESSION_ID` và `PPID` có sẵn trong hook context không? | High — xác định marker file có đúng PID không | Kiểm tra trong hook test (Q1) |
| Q6 | Zellij version hiện tại (0.44.2) có `SaveLayout` API không? | High — có thể feature mới hơn 0.44 | Check `zellij-tile` 0.44.x changelog |

---

## 10. Implementation Plan Outline (cho Tech Lead)

### Phase 0: Verify (2-4 giờ) — **BẮT BUỘC trước khi code**
- [ ] Verify Q1: Claude `SessionStart` hook behavior  
- [ ] Verify Q2: `SaveLayout` + restore với command/args  
- [ ] Verify Q4: Plugin pipe() background trigger  
- [ ] Quyết định go/no-go và điều chỉnh design nếu cần  

### Phase 1: MVP Plugin (2-3 ngày)
- [ ] Setup Cargo project: `wasm32-wasip1` target, `zellij-tile` dependency  
- [ ] Implement `pipe()` trigger: nhận save command  
- [ ] Implement `DumpSessionLayout` + parse response  
- [ ] Implement KDL enrichment với heuristic UUID (newest-jsonl)  
- [ ] Implement `SaveLayout` call  
- [ ] Build + test manually: `snap work-test` → verify file → `zellij --layout work-test`  

### Phase 2: UUID Accuracy (1-2 ngày)
- [ ] Setup Claude Code hook (nếu Q1 verified)  
- [ ] Implement marker file read trong plugin  
- [ ] Test với /resume UI: pick session, save, reboot, verify đúng session resume  
- [ ] Implement fallback chain: marker → heuristic → skip  

### Phase 3: Polish (1-2 ngày)
- [ ] Fish functions: `snap`, `snap-list`, `snap-load`  
- [ ] Plugin UI: hiển thị thời gian snapshot cuối (nếu cần)  
- [ ] Error handling: permission denied, malformed KDL, timeout  
- [ ] Auto-start plugin trong default layout  
- [ ] Optional: cron auto-save mỗi 30 phút  

### Phase 4: Hardening (1 ngày)
- [ ] Test multi-pane same-cwd edge case  
- [ ] Test Zellij version upgrade scenario  
- [ ] Document: README cho setup (hook + fish functions + plugin placement)  

**Total estimated effort**: 5-8 ngày engineer, solo.

---

## 11. Non-Goals (không làm trong scope này)

- Restore nvim/vim buffer state — đây là vấn đề riêng, giải bằng `persistence.nvim`
- Auto-resurrect process state (CRIU) — không khả thi portable
- Support tmux (chỉ Zellij)
- Multi-user / shared sessions
- Cloud sync của snapshots

---

## 12. Tech Stack Summary

| Component | Tech | Notes |
|---|---|---|
| Zellij Plugin | Rust 1.95+, WASM target `wasm32-wasip1` | Compile với `cargo build --release --target wasm32-wasip1` |
| Plugin SDK | `zellij-tile` crate | Phải match version Zellij đang dùng (0.44.2) |
| KDL parsing | `kdl` crate | Hoặc regex thô nếu muốn zero-dep |
| Claude hook | JSON trong `~/.claude/settings.json` | Shell command viết marker file |
| CLI interface | Fish functions | Có thể port sang bash/zsh nếu cần |
| Zellij version | 0.44.2 (aqua, mise-managed) | `mise exec -- zellij --version` |
| Rust toolchain | rustup stable 1.95.0 | `wasm32-wasip1` target đã add |

---

## 13. References — Source đã đọc

| File | Relevance |
|---|---|
| `zellij-server/src/background_jobs.rs:133-218` | 3 background loops: serialize, cwd refresh, metadata write |
| `zellij-server/src/background_jobs.rs:667-703` | `write_session_state_to_disk`: logic ghi file + content-hash check |
| `zellij-server/src/background_jobs.rs:706-738` | `scan_session_list` + `find_resurrectable_sessions` |
| `zellij-server/src/os_input_output.rs:494-536` | `get_cwd`, `get_cwds`: đọc `/proc/<pid>/cwd` qua sysinfo |
| `zellij-server/src/pty.rs:1950-1988` | `UpdateAndReportCwds`: cách Zellij update cwd mỗi giây |
| `zellij-server/src/panes/grid.rs:3318-3356` | `pane_contents`: dump scrollback thành String |
| `zellij-utils/src/consts.rs:28-108` | Cache paths, session info folder structure |
| `zellij-utils/src/session_serialization.rs:167-232` | KDL serialization của command + args + cwd |
| `zellij-utils/src/plugin_api/plugin_command.proto:930-936` | `OverrideLayoutPayload` shape |
| `zellij-utils/src/plugin_api/plugin_command.proto:169-175` | `SaveLayout`, `OverrideLayout`, `EditLayout`, `ParseLayout` command types |
| `zellij-utils/src/plugin_api/event.proto` | 47 event types — confirmed không có `BeforeSerializeLayout` |
| `~/.claude/projects/-home-thaivro/*.jsonl` | JSONL format thực tế: record types, `parentUuid`, `sessionId` |
| `~/.claude/projects/-home-thaivro/3939d9e8-*.jsonl` | Session đang mở, 297KB, confirmed format |
