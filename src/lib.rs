use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    pending_snapshot_name: Option<String>,
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::WriteToStdin,
        ]);
        subscribe(&[EventType::CustomMessage, EventType::PermissionRequestResult]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::CustomMessage(name, payload) if name == "session_layout" => {
                if let Some(snapshot_name) = self.pending_snapshot_name.take() {
                    eprintln!("[zellij-claude-sync] saving snapshot: {}", snapshot_name);
                    // TODO: enrich KDL with session IDs, then SaveLayout
                    save_layout(&snapshot_name, &payload, true);
                }
            }
            _ => {}
        }
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if pipe_message.name == "save" {
            let name = pipe_message
                .payload
                .unwrap_or_else(|| "unnamed".to_string());
            eprintln!("[zellij-claude-sync] trigger save: {}", name);
            self.pending_snapshot_name = Some(name);
            dump_session_layout();
        }
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {}
}

register_plugin!(State);
