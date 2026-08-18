//! Python REPL plugin for Open CAD Studio (API v4).
//!
//! Dispatches `PYTHONSHELL` to spawn an IPython REPL in an OS terminal window
//! bound to the active document tab. The Python child reads the live document
//! from the host V4 shared-memory snapshot and mutates it through the plugin API
//! request proxy.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use ocs_plugin_api::export_plugin;
use ocs_plugin_api::host::{BuiltinPlugin, HostApi, HostNotification};
use ocs_plugin_api::manifest::PluginManifest;
use ocs_plugin_api::ribbon::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};
use pyo3::prelude::*;

pub mod alloc;
pub mod document;
pub mod platform;
mod python_ext;
mod repl;
pub mod session_dir;

use crate::repl::ReplSession;

static MANIFEST: PluginManifest = PluginManifest {
    id: "opencad.python_repl",
    name: "Python REPL",
    version: "0.1.0",
    description: "Interactive Python REPL for Open CAD Studio.",
    api_version: ocs_plugin_api::manifest::ApiVersion { major: 4 },
    ribbon_order: 70,
    xdata_apps: &["PYREPL"],
    command_prefixes: &["PYTHONSHELL"],
};

struct PythonReplPlugin {
    sessions: Arc<Mutex<HashMap<u64, RefCell<ReplSession>>>>,
}

impl PythonReplPlugin {
    fn new() -> Self {
        crate::document::debug_log("PythonReplPlugin::new");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn ensure_session(&self, host: &mut dyn HostApi) {
        crate::document::debug_log("PythonReplPlugin::ensure_session start");
        let tab_id = host.tab_id();
        crate::document::debug_log(&format!("PythonReplPlugin::ensure_session tab_id={tab_id}"));
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cell) = sessions.get(&tab_id) {
            if cell.borrow_mut().is_alive() {
                host.push_info("Python REPL is already running for this tab.");
                return;
            }
        }
        drop(sessions);

        eprintln!("[python-repl] spawning session for tab {tab_id}");
        crate::document::debug_log("PythonReplPlugin::ensure_session about to spawn");
        match ReplSession::spawn(host, tab_id) {
            Ok(session) => {
                crate::document::debug_log("PythonReplPlugin::ensure_session spawned OK");
                host.push_info("Python REPL started.");
                self.sessions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(tab_id, RefCell::new(session));
                crate::document::debug_log("PythonReplPlugin::ensure_session inserted session");
            }
            Err(e) => {
                let msg = format!("failed to start Python REPL: {e}");
                eprintln!("[python-repl] {msg}");
                host.push_error(&msg);
            }
        }
        crate::document::debug_log("PythonReplPlugin::ensure_session end");
    }
}

impl Drop for PythonReplPlugin {
    fn drop(&mut self) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let tab_ids: Vec<u64> = sessions.keys().copied().collect();
        for tab_id in tab_ids {
            if let Some(cell) = sessions.remove(&tab_id) {
                let _ = cell.into_inner();
            }
        }
    }
}

impl BuiltinPlugin for PythonReplPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }

    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(ReplRibbonModule)
    }

    fn dispatch(&self, host: &mut dyn HostApi, cmd: &str) -> bool {
        if cmd == "PYTHONSHELL" {
            self.ensure_session(host);
            true
        } else {
            false
        }
    }

    fn on_notification(&mut self, _command_id: Option<u64>, notification: HostNotification) {
        match notification {
            HostNotification::DocumentTabClosed { tab_id } => {
                if let Some(cell) = self
                    .sessions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&tab_id)
                {
                    // Dropping ReplSession kills the Python process group and
                    // joins the proxy thread; do it off the reader thread.
                    std::thread::spawn(move || {
                        let _ = cell.into_inner();
                    });
                }
            }
            // DocumentChangedV4 is ignored: the Python side must call
            // `ocs.doc.refresh()` to see host updates.
            HostNotification::DocumentChangedV4 { .. } => {}
            // Cancel is a no-op; the user interrupts IPython with Ctrl+C in the
            // terminal window.
            HostNotification::Cancel => {}
            _ => {}
        }
    }
}

struct ReplRibbonModule;

impl CadModule for ReplRibbonModule {
    fn id(&self) -> &'static str {
        MANIFEST.id
    }
    fn title(&self) -> &'static str {
        MANIFEST.name
    }
    fn ribbon_groups(&self) -> &[RibbonGroup] {
        static GROUPS: OnceLock<Vec<RibbonGroup>> = OnceLock::new();
        GROUPS.get_or_init(|| {
            vec![RibbonGroup {
                title: "Scripting",
                tools: vec![RibbonItem::LargeTool(ToolDef {
                    id: "PYTHONSHELL",
                    label: "Python Shell",
                    icon: IconKind::Svg(include_bytes!("../assets/python.svg")),
                    event: ModuleEvent::Command("PYTHONSHELL".to_string()),
                })],
            }]
        })
    }
}

#[pymodule]
fn ocs_python_repl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    python_ext::init_module(m)?;
    Ok(())
}

export_plugin!(PythonReplPlugin::new());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_advertises_v4_and_pythonshell() {
        let plugin = PythonReplPlugin::new();
        let m = plugin.manifest();
        assert_eq!(m.api_version.major, 4);
        assert!(m.command_prefixes.contains(&"PYTHONSHELL"));
    }

    #[test]
    fn ribbon_advertises_pythonshell_tool() {
        use ocs_plugin_api::ribbon::{IconKind, RibbonItem};
        let plugin = PythonReplPlugin::new();
        let module = plugin.ribbon();
        let groups = module.ribbon_groups();
        assert_eq!(groups.len(), 1, "expected one ribbon group");
        assert_eq!(groups[0].title, "Scripting");
        assert_eq!(groups[0].tools.len(), 1, "expected one tool in the group");
        let RibbonItem::LargeTool(ref tool) = groups[0].tools[0] else {
            panic!("expected a large tool");
        };
        assert_eq!(tool.id, "PYTHONSHELL");
        assert_eq!(tool.label, "Python Shell");
        assert!(matches!(tool.icon, IconKind::Svg(_)));
        assert!(
            matches!(tool.event, ocs_plugin_api::ribbon::ModuleEvent::Command(ref cmd) if cmd == "PYTHONSHELL")
        );
    }

    #[test]
    fn entity_type_bincode_json_roundtrip() {
        use acadrust::entities::Point;
        use acadrust::EntityType;

        let mut point = Point::from_coords(1.0, 2.0, 3.0);
        point.common.layer = "TEST".to_string();
        let entity = EntityType::Point(point);

        let bin = bincode::serialize(&entity).unwrap();
        let decoded: EntityType = bincode::deserialize(&bin).unwrap();
        assert!(matches!(decoded, EntityType::Point(_)));

        let json = serde_json::to_string(&entity).unwrap();
        let decoded_json: EntityType = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded_json, EntityType::Point(_)));
    }

    #[test]
    fn generated_python_files_exist_and_are_valid() {
        use crate::session_dir::SessionDir;
        let dir = SessionDir::create(5555).unwrap();
        let entities = std::fs::read_to_string(dir.root().join("ocs/entities.py")).unwrap();
        assert!(entities.contains("class Point(Entity)"));
        assert!(entities.contains("@dataclass"));
        let startup = std::fs::read_to_string(dir.root().join("startup.py")).unwrap();
        assert!(startup.contains("def pyimport"));
        assert!(startup.contains("def pyexport"));
        let wrapper = std::fs::read_to_string(dir.root().join("repl_wrapper.py")).unwrap();
        assert!(wrapper.contains("IPython"));
        std::fs::read_to_string(dir.root().join("ocs/entities.pyi")).unwrap();
        std::fs::read_to_string(dir.root().join("ocs/__init__.pyi")).unwrap();
        std::fs::read(dir.root().join("py.typed")).unwrap();
        dir.delete().unwrap();
    }
}
