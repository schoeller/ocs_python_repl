//! Python-facing document view backed by the host's V4 shared snapshot.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use acadrust::{EntityType, Handle};
use ocs_plugin_api::host::PluginRequestError;
use ocs_plugin_api::ipc::protocol::{PluginRequest, PluginResponse};
use ocs_plugin_api::ipc::proxy::{ProxyPluginRequestSender, PROXY_TOKEN_LEN};
use ocs_plugin_api::shm::{DocumentViewDataV4, SharedDocumentReader};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::python_ext::{entity_to_py, py_to_entity, py_to_xdata_record, xdata_record_to_py};

fn parse_token(s: String) -> Option<[u8; PROXY_TOKEN_LEN]> {
    if s.len() != 2 * PROXY_TOKEN_LEN {
        return None;
    }
    let mut out = [0u8; PROXY_TOKEN_LEN];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(out)
}

/// Per-tab document handle exposed to Python as `ocs.doc`.
#[pyclass]
pub struct Document {
    reader: Mutex<SharedDocumentReader<DocumentViewDataV4>>,
    proxy: Option<Arc<ProxyPluginRequestSender>>,
    handle_index: Mutex<HashMap<u64, usize>>,
}

fn require_sender(doc: &Document) -> PyResult<Arc<ProxyPluginRequestSender>> {
    if let Some(proxy) = doc.proxy.as_ref() {
        return Ok(Arc::clone(proxy));
    }
    Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
        "not connected to the OpenCADStudio host. \
Run PYTHONSHELL from the host to mutate the document; \
standalone Python processes only have read-only access to the snapshot."
            .to_string(),
    ))
}

#[pymethods]
impl Document {
    #[new]
    #[pyo3(signature = (snapshot_path))]
    pub fn new(snapshot_path: String) -> PyResult<Self> {
        let reader = SharedDocumentReader::<DocumentViewDataV4>::open(Path::new(&snapshot_path))
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let proxy = match std::env::var("OCS_REQUEST_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
        {
            Some(port) => {
                let token = std::env::var("OCS_REQUEST_TOKEN")
                    .ok()
                    .and_then(parse_token)
                    .ok_or_else(|| {
                        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                            "OCS_REQUEST_TOKEN missing or invalid".to_string(),
                        )
                    })?;
                let result =
                    ProxyPluginRequestSender::connect_with_token("127.0.0.1", port, &token);
                if let Err(ref e) = result {
                    eprintln!("[python-repl] request proxy connect failed: {e}");
                }
                result.ok()
            }
            None => None,
        };
        Ok(Self {
            reader: Mutex::new(reader),
            proxy: proxy.map(Arc::new),
            handle_index: Mutex::new(HashMap::new()),
        })
    }

    /// Refresh the cached snapshot version if the host has published a newer one.
    fn refresh(&self) -> PyResult<()> {
        let mut reader = self
            .reader
            .lock()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        if !reader.has_new_version() {
            return Ok(());
        }
        reader.refresh();
        if let Some(archived) = reader.payload() {
            let mut index = self
                .handle_index
                .lock()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
            index.clear();
            for (i, e) in archived.entities.iter().enumerate() {
                index.insert(e.handle, i);
            }
        }
        Ok(())
    }

    fn entities(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let reader = self
            .reader
            .lock()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let archived = reader.payload().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("no archived document")
        })?;
        archived
            .entities
            .iter()
            .map(|e| {
                let entity: EntityType = bincode::deserialize(&e.data).map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
                })?;
                entity_to_py(py, &entity, e.handle)
            })
            .collect()
    }

    fn entity(&self, py: Python<'_>, handle: u64) -> PyResult<Option<PyObject>> {
        let reader = self
            .reader
            .lock()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let archived = reader.payload().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("no archived document")
        })?;
        let index = self
            .handle_index
            .lock()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let Some(&idx) = index.get(&handle) else {
            return Ok(None);
        };
        let e = &archived.entities[idx];
        let entity: EntityType = bincode::deserialize(&e.data)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        Ok(Some(entity_to_py(py, &entity, e.handle)?))
    }

    fn layers(&self) -> PyResult<HashMap<u64, String>> {
        let reader = self
            .reader
            .lock()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let archived = reader.payload().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("no archived document")
        })?;
        Ok(archived
            .layers
            .iter()
            .map(|l| (l.handle, l.name.to_string()))
            .collect())
    }

    fn counts(&self) -> PyResult<HashMap<String, usize>> {
        let mut counts = HashMap::new();
        let reader = self
            .reader
            .lock()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let archived = reader.payload().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("no archived document")
        })?;
        for e in archived.entities.iter() {
            let entity: EntityType = bincode::deserialize(&e.data)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
            let name = match entity {
                EntityType::Point(_) => "Point",
                EntityType::Line(_) => "Line",
                EntityType::Circle(_) => "Circle",
                EntityType::Arc(_) => "Arc",
                EntityType::Polyline(_) => "Polyline",
                EntityType::Polyline2D(_) => "Polyline2D",
                EntityType::Polyline3D(_) => "Polyline3D",
                EntityType::LwPolyline(_) => "LwPolyline",
                EntityType::Spline(_) => "Spline",
                EntityType::MText(_) => "MText",
                _ => "Other",
            };
            *counts.entry(name.to_string()).or_insert(0) += 1;
        }
        Ok(counts)
    }

    fn add<'py>(&self, py: Python<'py>, entity: &Bound<'py, PyDict>) -> PyResult<u64> {
        let entity = py_to_entity(entity)?;
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(PluginRequest::AddEntity(entity), &mut || {
            if let Err(e) = py.check_signals() {
                *interrupt.borrow_mut() = Some(e);
                return Err(ocs_plugin_api::host::PluginRequestError(
                    "interrupted".to_string(),
                ));
            }
            Ok(())
        });
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Handle(h)) => Ok(h.value()),
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected add response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }

    fn add_many<'py>(
        &self,
        py: Python<'py>,
        entities: &Bound<'py, pyo3::types::PyList>,
    ) -> PyResult<Vec<u64>> {
        let entities: Vec<EntityType> = entities
            .iter()
            .map(|item| {
                let dict = item.downcast::<PyDict>().map_err(|_| {
                    PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                        "add_many expects a list of entity dicts",
                    )
                })?;
                py_to_entity(dict)
            })
            .collect::<PyResult<_>>()?;
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(PluginRequest::AddEntities(entities), &mut || {
            if let Err(e) = py.check_signals() {
                *interrupt.borrow_mut() = Some(e);
                return Err(ocs_plugin_api::host::PluginRequestError(
                    "interrupted".to_string(),
                ));
            }
            Ok(())
        });
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Handles(handles)) => {
                Ok(handles.into_iter().map(|h| h.value()).collect())
            }
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected add_many response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }

    fn update<'py>(&self, py: Python<'py>, entity: &Bound<'py, PyDict>) -> PyResult<bool> {
        let entity = py_to_entity(entity)?;
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(PluginRequest::UpdateEntity(entity), &mut || {
            if let Err(e) = py.check_signals() {
                *interrupt.borrow_mut() = Some(e);
                return Err(ocs_plugin_api::host::PluginRequestError(
                    "interrupted".to_string(),
                ));
            }
            Ok(())
        });
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Bool(b)) => Ok(b),
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected update response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }

    fn remove<'py>(&self, py: Python<'py>, handle: u64) -> PyResult<bool> {
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(
            PluginRequest::RemoveEntity {
                handle: Handle::new(handle),
            },
            &mut || {
                if let Err(e) = py.check_signals() {
                    *interrupt.borrow_mut() = Some(e);
                    return Err(ocs_plugin_api::host::PluginRequestError(
                        "interrupted".to_string(),
                    ));
                }
                Ok(())
            },
        );
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Bool(b)) => Ok(b),
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected remove response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }

    /// Read the XDATA record attached to `handle` for application `app_name`.
    /// Returns `None` if the entity has no such record.
    fn read_record<'py>(
        &self,
        py: Python<'py>,
        handle: u64,
        app_name: String,
    ) -> PyResult<Option<PyObject>> {
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(
            PluginRequest::ReadRecord {
                handle: Handle::new(handle),
                app_name,
            },
            &mut || {
                if let Err(e) = py.check_signals() {
                    *interrupt.borrow_mut() = Some(e);
                    return Err(ocs_plugin_api::host::PluginRequestError(
                        "interrupted".to_string(),
                    ));
                }
                Ok(())
            },
        );
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Record(record)) => {
                record.map(|r| xdata_record_to_py(py, &r)).transpose()
            }
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected read_record response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }

    /// Write an XDATA record to `handle`. `record` must be a dict with
    /// `app_name` (str) and `values` (list of `{kind, value}` dicts).
    fn write_record<'py>(
        &self,
        py: Python<'py>,
        handle: u64,
        record: &Bound<'py, PyDict>,
    ) -> PyResult<bool> {
        let record = py_to_xdata_record(record)?;
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(
            PluginRequest::WriteRecord {
                handle: Handle::new(handle),
                record,
            },
            &mut || {
                if let Err(e) = py.check_signals() {
                    *interrupt.borrow_mut() = Some(e);
                    return Err(ocs_plugin_api::host::PluginRequestError(
                        "interrupted".to_string(),
                    ));
                }
                Ok(())
            },
        );
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Bool(b)) => Ok(b),
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected write_record response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }

    /// Remove the XDATA record for `app_name` from `handle`.
    fn remove_record<'py>(&self, py: Python<'py>, handle: u64, app_name: String) -> PyResult<bool> {
        let sender = require_sender(self)?;
        let interrupt: RefCell<Option<PyErr>> = RefCell::new(None);
        let result = sender.request_with_poll(
            PluginRequest::RemoveRecord {
                handle: Handle::new(handle),
                app_name,
            },
            &mut || {
                if let Err(e) = py.check_signals() {
                    *interrupt.borrow_mut() = Some(e);
                    return Err(ocs_plugin_api::host::PluginRequestError(
                        "interrupted".to_string(),
                    ));
                }
                Ok(())
            },
        );
        if let Some(e) = interrupt.into_inner() {
            return Err(e);
        }
        match result {
            Ok(PluginResponse::Bool(b)) => Ok(b),
            Ok(other) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "unexpected remove_record response: {other:?}"
            ))),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                PluginRequestError::to_string(&e),
            )),
        }
    }
}
