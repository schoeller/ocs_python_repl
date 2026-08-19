//! PyO3 `_ocs` extension module: document binding and entity conversion.

use acadrust::entities::Entity as EntityTrait;
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::EntityType;
use pyo3::conversion::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

#[cfg(test)]
use acadrust::entities::{
    Arc as CadArc, Circle, Ellipse, Line, LwPolyline, LwVertex, MText, Point, Polyline,
    Polyline2D, Polyline3D, Spline,
};

use crate::document::Document;

/// Emit a Python `warnings.warn` for a type mismatch in an entity dict.
fn warn_type_mismatch(bound: &Bound<'_, PyAny>, key: &str, expected: &str) {
    let py = bound.py();
    let msg = format!("expected {expected} for '{key}', using default");
    let _ = py
        .import("warnings")
        .and_then(|m| m.getattr("warn"))
        .and_then(|w| w.call1((msg,)));
}

/// Initialize the `ocs_python_repl` extension module.
pub fn init_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    crate::document::debug_log("init_module start");
    crate::alloc::init_log();
    crate::alloc::probe_allocator();
    m.add_class::<Document>()?;
    crate::document::debug_log("init_module Document class added");
    m.add_wrapped(wrap_pyfunction!(_init))?;
    crate::document::debug_log("init_module _init function added");
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (snapshot_path))]
fn _init(snapshot_path: String) -> PyResult<Document> {
    crate::document::debug_log("_init called");
    Document::new(snapshot_path)
}

// ═══════════════════════════════════════════════════════════════════════════
// Entity -> Python
// ═══════════════════════════════════════════════════════════════════════════

fn base_kwargs(py: Python<'_>, layer: String) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("layer", layer)?;
    Ok(dict)
}

fn v3_tuple(v: &acadrust::types::Vector3) -> (f64, f64, f64) {
    (v.x, v.y, v.z)
}

fn smooth_surface_name_3d(t: &acadrust::entities::polyline3d::SmoothSurfaceType) -> &'static str {
    match t {
        acadrust::entities::polyline3d::SmoothSurfaceType::None => "None",
        acadrust::entities::polyline3d::SmoothSurfaceType::QuadraticBSpline => "QuadraticBSpline",
        acadrust::entities::polyline3d::SmoothSurfaceType::CubicBSpline => "CubicBSpline",
        acadrust::entities::polyline3d::SmoothSurfaceType::Bezier => "Bezier",
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Entity -> Python (generated at build time from crud_manifest.json).
// ═══════════════════════════════════════════════════════════════════════════

include!(concat!(env!("OUT_DIR"), "/entity_crud.rs"));

// ═══════════════════════════════════════════════════════════════════════════
// Python -> Entity
// ═══════════════════════════════════════════════════════════════════════════

fn get_opt_string(entity: &Bound<'_, PyAny>, key: &str) -> PyResult<String> {
    let Some(value) = entity_attr(entity, key) else {
        return Ok(String::new());
    };
    value.extract::<String>()
}

fn get_opt_f64(entity: &Bound<'_, PyAny>, key: &str, default: f64) -> f64 {
    let Some(value) = entity_attr(entity, key) else {
        return default;
    };
    match value.extract::<f64>() {
        Ok(v) => v,
        Err(_) => {
            warn_type_mismatch(&value, key, "float");
            default
        }
    }
}

fn get_opt_u64(entity: &Bound<'_, PyAny>, key: &str, default: u64) -> u64 {
    let Some(value) = entity_attr(entity, key) else {
        return default;
    };
    match value.extract::<u64>() {
        Ok(v) => v,
        Err(_) => {
            warn_type_mismatch(&value, key, "int");
            default
        }
    }
}

fn get_opt_bool(entity: &Bound<'_, PyAny>, key: &str, default: bool) -> bool {
    let Some(value) = entity_attr(entity, key) else {
        return default;
    };
    match value.extract::<bool>() {
        Ok(v) => v,
        Err(_) => {
            warn_type_mismatch(&value, key, "bool");
            default
        }
    }
}

/// Extract a Vector3 from a tuple or an object with x/y/z attributes.
fn py_vector3(obj: &Bound<'_, PyAny>) -> PyResult<acadrust::types::Vector3> {
    if let Ok(t) = obj.extract::<(f64, f64, f64)>() {
        return Ok(acadrust::types::Vector3::new(t.0, t.1, t.2));
    }
    let x = obj.getattr("x")?.extract::<f64>()?;
    let y = obj.getattr("y")?.extract::<f64>()?;
    let z = obj.getattr("z")?.extract::<f64>()?;
    Ok(acadrust::types::Vector3::new(x, y, z))
}

fn py_vector3_opt(
    entity: &Bound<'_, PyAny>,
    key: &str,
    default: acadrust::types::Vector3,
) -> PyResult<acadrust::types::Vector3> {
    let Some(value) = entity_attr(entity, key) else {
        return Ok(default);
    };
    py_vector3(&value)
}

fn py_vector2(entity: &Bound<'_, PyAny>) -> PyResult<acadrust::types::Vector2> {
    if let Ok(t) = entity.extract::<(f64, f64)>() {
        return Ok(acadrust::types::Vector2::new(t.0, t.1));
    }
    let x = entity.getattr("x")?.extract::<f64>()?;
    let y = entity.getattr("y")?.extract::<f64>()?;
    Ok(acadrust::types::Vector2::new(x, y))
}

fn point_list_2d(obj: &Bound<'_, PyAny>) -> PyResult<Vec<acadrust::types::Vector2>> {
    let list = obj.downcast::<PyList>()?;
    list.iter().map(|item| py_vector2(&item)).collect()
}

fn smooth_surface_type_3d(
    entity: &Bound<'_, PyAny>,
    key: &str,
) -> acadrust::entities::polyline3d::SmoothSurfaceType {
    match get_opt_string(entity, key).unwrap_or_default().as_str() {
        "QuadraticBSpline" => acadrust::entities::polyline3d::SmoothSurfaceType::QuadraticBSpline,
        "CubicBSpline" => acadrust::entities::polyline3d::SmoothSurfaceType::CubicBSpline,
        "Bezier" => acadrust::entities::polyline3d::SmoothSurfaceType::Bezier,
        _ => acadrust::entities::polyline3d::SmoothSurfaceType::None,
    }
}

fn point_list(obj: &Bound<'_, PyAny>) -> PyResult<Vec<acadrust::types::Vector3>> {
    let list = obj.downcast::<PyList>()?;
    list.iter().map(|item| py_vector3(&item)).collect()
}

fn f64_list(list: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    let list = list.downcast::<PyList>().map_err(|_| {
        PyErr::new::<pyo3::exceptions::PyTypeError, _>("expected a list of floats")
    })?;
    list.iter()
        .map(|item| item.extract::<f64>())
        .collect::<PyResult<Vec<_>>>()
}

fn set_common<T: EntityTrait>(e: &mut T, entity: &Bound<'_, PyAny>) -> PyResult<()> {
    let handle = get_opt_u64(entity, "handle", 0);
    let layer = get_opt_string(entity, "layer")?;
    e.set_handle(acadrust::Handle::new(handle));
    e.set_layer(layer);
    Ok(())
}

/// Return an attribute if `entity` is an object, or a dict item if it is a dict.
/// Returns `None` if the attribute is missing **or** if its value is Python `None`.
///
/// This helper is referenced by generated code in `entity_crud.rs` whenever the
/// registry contains an `Option<T>` field, so it may be unused for some registry
/// snapshots. Keep it available for the generator.
#[allow(dead_code)]
fn opt_entity_attr<'py>(entity: &Bound<'py, PyAny>, key: &str) -> Option<Bound<'py, PyAny>> {
    entity_attr(entity, key).filter(|v| !v.is_none())
}

/// Return an attribute if `entity` is an object, or a dict item if it is a dict.
fn entity_attr<'py>(entity: &Bound<'py, PyAny>, key: &str) -> Option<Bound<'py, PyAny>> {
    if let Ok(dict) = entity.downcast::<PyDict>() {
        if let Ok(Some(v)) = dict.get_item(key) {
            return Some(v);
        }
    }
    entity.getattr(key).ok()
}

/// Determine the entity kind from a "kind" dict item or the object's class name.
fn entity_kind(entity: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Some(kind) = entity_attr(entity, "kind") {
        if let Ok(s) = kind.extract::<String>() {
            return Ok(s);
        }
    }
    entity.getattr("__class__")?.getattr("__name__")?.extract()
}

// ═══════════════════════════════════════════════════════════════════════════
// XDATA -> Python
// ═══════════════════════════════════════════════════════════════════════════

/// Convert an `acadrust::xdata::ExtendedDataRecord` into a Python dict:
/// `{ "app_name": str, "values": [{ "kind": str, "value": ... }, ...] }`.
pub fn xdata_record_to_py(py: Python<'_>, record: &ExtendedDataRecord) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("app_name", record.application_name.clone())?;
    let values = PyList::empty(py);
    for v in &record.values {
        values.append(xdata_value_to_py(py, v)?)?;
    }
    dict.set_item("values", values)?;
    Ok(dict.into())
}

fn xdata_value_to_py(py: Python<'_>, value: &XDataValue) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    let (kind, py_value): (&str, PyObject) = match value {
        XDataValue::String(s) => ("String", s.clone().into_py_any(py)?),
        XDataValue::ControlString(s) => ("ControlString", s.clone().into_py_any(py)?),
        XDataValue::LayerName(s) => ("LayerName", s.clone().into_py_any(py)?),
        XDataValue::BinaryData(b) => ("BinaryData", b.clone().into_py_any(py)?),
        XDataValue::Handle(h) => ("Handle", h.value().into_py_any(py)?),
        XDataValue::Point3D(v) => ("Point3D", (v.x, v.y, v.z).into_py_any(py)?),
        XDataValue::Position3D(v) => ("Position3D", (v.x, v.y, v.z).into_py_any(py)?),
        XDataValue::Displacement3D(v) => ("Displacement3D", (v.x, v.y, v.z).into_py_any(py)?),
        XDataValue::Direction3D(v) => ("Direction3D", (v.x, v.y, v.z).into_py_any(py)?),
        XDataValue::Real(r) => ("Real", (*r).into_py_any(py)?),
        XDataValue::Distance(d) => ("Distance", (*d).into_py_any(py)?),
        XDataValue::ScaleFactor(s) => ("ScaleFactor", (*s).into_py_any(py)?),
        XDataValue::Integer16(i) => ("Integer16", (*i).into_py_any(py)?),
        XDataValue::Integer32(i) => ("Integer32", (*i).into_py_any(py)?),
    };
    dict.set_item("kind", kind)?;
    dict.set_item("value", py_value)?;
    Ok(dict.into())
}

// ═══════════════════════════════════════════════════════════════════════════
// Python -> XDATA
// ═══════════════════════════════════════════════════════════════════════════

/// Convert a Python dict `{ "app_name": str, "values": [...] }` into an
/// `acadrust::xdata::ExtendedDataRecord`.
pub fn py_to_xdata_record(record: &Bound<'_, PyDict>) -> PyResult<ExtendedDataRecord> {
    let app_name = record
        .get_item("app_name")?
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "XDATA record dict missing 'app_name'".to_string(),
            )
        })?
        .extract::<String>()?;

    let mut extended = ExtendedDataRecord::new(app_name);

    let values_obj = record.get_item("values")?.ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "XDATA record dict missing 'values' list".to_string(),
        )
    })?;
    let values = values_obj.downcast::<PyList>().map_err(|_| {
        PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "XDATA record 'values' must be a list".to_string(),
        )
    })?;

    for item in values.iter() {
        let dict = item.downcast::<PyDict>().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "each XDATA value must be a dict with 'kind' and 'value'".to_string(),
            )
        })?;
        extended.add_value(py_to_xdata_value(dict)?);
    }

    Ok(extended)
}

fn py_to_xdata_value(dict: &Bound<'_, PyDict>) -> PyResult<XDataValue> {
    let kind: String = dict
        .get_item("kind")?
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "XDATA value dict missing 'kind'".to_string(),
            )
        })?
        .extract()?;
    let value = dict.get_item("value")?.ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "XDATA value dict missing 'value'".to_string(),
        )
    })?;

    match kind.as_str() {
        "String" => Ok(XDataValue::String(value.extract()?)),
        "ControlString" => Ok(XDataValue::ControlString(value.extract()?)),
        "LayerName" => Ok(XDataValue::LayerName(value.extract()?)),
        "BinaryData" => Ok(XDataValue::BinaryData(value.extract()?)),
        "Handle" => Ok(XDataValue::Handle(acadrust::Handle::new(value.extract()?))),
        "Point3D" => Ok(XDataValue::Point3D(py_vector3(&value)?)),
        "Position3D" => Ok(XDataValue::Position3D(py_vector3(&value)?)),
        "Displacement3D" => Ok(XDataValue::Displacement3D(py_vector3(&value)?)),
        "Direction3D" => Ok(XDataValue::Direction3D(py_vector3(&value)?)),
        "Real" => Ok(XDataValue::Real(value.extract()?)),
        "Distance" => Ok(XDataValue::Distance(value.extract()?)),
        "ScaleFactor" => Ok(XDataValue::ScaleFactor(value.extract()?)),
        "Integer16" => Ok(XDataValue::Integer16(value.extract()?)),
        "Integer32" => Ok(XDataValue::Integer32(value.extract()?)),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unsupported XDATA value kind: {kind}; supported: String, ControlString, LayerName, \
             BinaryData, Handle, Point3D, Position3D, Displacement3D, Direction3D, Real, Distance, \
             ScaleFactor, Integer16, Integer32"
        ))),
    }
}

// `py_to_entity` is generated at build time in `entity_crud.rs`.

#[cfg(test)]
mod roundtrip_tests {
    use super::*;
    use acadrust::Handle;

    fn roundtrip(py: Python<'_>, entity: EntityType) -> PyResult<EntityType> {
        // Load the generated `ocs/entities.py` dataclasses without importing the
        // full `ocs` package (which requires the compiled Rust extension).
        let code = include_str!(concat!(env!("OUT_DIR"), "/python/ocs/entities.py"));
        #[allow(deprecated)]
        let entities_mod = PyModule::from_code_bound(py, code, "entities.py", "ocs.entities")?;
        let ocs_mod = PyModule::new(py, "ocs")?;
        ocs_mod.setattr("entities", &entities_mod)?;
        py.import("sys")?
            .getattr("modules")?
            .set_item("ocs", &ocs_mod)?;
        py.import("sys")?
            .getattr("modules")?
            .set_item("ocs.entities", &entities_mod)?;

        let handle = entity.common().handle.value();
        let obj = entity_to_py(py, &entity, handle)?;
        let bound = obj.bind(py);
        py_to_entity(bound)
    }

    fn assert_v3_eq(a: &acadrust::types::Vector3, b: &acadrust::types::Vector3) {
        assert!((a.x - b.x).abs() < 1e-9, "x mismatch: {} vs {}", a.x, b.x);
        assert!((a.y - b.y).abs() < 1e-9, "y mismatch: {} vs {}", a.y, b.y);
        assert!((a.z - b.z).abs() < 1e-9, "z mismatch: {} vs {}", a.z, b.z);
    }

    fn assert_f64_eq(a: f64, b: f64, msg: &str) {
        assert!((a - b).abs() < 1e-9, "{msg}: {a} vs {b}");
    }

    #[test]
    fn point_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut p = Point::from_coords(1.0, 2.0, 3.0);
            p.common.handle = Handle::new(42);
            p.common.layer = "L1".to_string();
            p.thickness = 5.0;
            p.normal = acadrust::types::Vector3::new(0.0, 0.0, 1.0);
            p.x_axis_angle = 0.5;

            let rt = roundtrip(py, EntityType::Point(p)).unwrap();
            if let EntityType::Point(q) = rt {
                assert_eq!(q.common.handle.value(), 42);
                assert_eq!(q.common.layer, "L1");
                assert_f64_eq(q.location.x, 1.0, "point x");
                assert_f64_eq(q.location.y, 2.0, "point y");
                assert_f64_eq(q.location.z, 3.0, "point z");
                assert_f64_eq(q.thickness, 5.0, "point thickness");
                assert_f64_eq(q.x_axis_angle, 0.5, "point x_axis_angle");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn line_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut l = Line::new();
            l.common.handle = Handle::new(7);
            l.start = acadrust::types::Vector3::new(0.0, 0.0, 0.0);
            l.end = acadrust::types::Vector3::new(10.0, 20.0, 30.0);
            l.thickness = 2.0;

            let rt = roundtrip(py, EntityType::Line(l)).unwrap();
            if let EntityType::Line(m) = rt {
                assert_eq!(m.common.handle.value(), 7);
                assert_v3_eq(&m.start, &acadrust::types::Vector3::new(0.0, 0.0, 0.0));
                assert_v3_eq(&m.end, &acadrust::types::Vector3::new(10.0, 20.0, 30.0));
                assert_f64_eq(m.thickness, 2.0, "line thickness");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn circle_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut c = Circle::new();
            c.common.handle = Handle::new(8);
            c.center = acadrust::types::Vector3::new(5.0, 6.0, 7.0);
            c.radius = 12.5;
            c.thickness = 1.0;

            let rt = roundtrip(py, EntityType::Circle(c)).unwrap();
            if let EntityType::Circle(d) = rt {
                assert_eq!(d.common.handle.value(), 8);
                assert_v3_eq(&d.center, &acadrust::types::Vector3::new(5.0, 6.0, 7.0));
                assert_f64_eq(d.radius, 12.5, "circle radius");
                assert_f64_eq(d.thickness, 1.0, "circle thickness");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn arc_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut a = CadArc::new();
            a.common.handle = Handle::new(9);
            a.center = acadrust::types::Vector3::new(1.0, 2.0, 0.0);
            a.radius = 5.0;
            a.start_angle = 0.0;
            a.end_angle = 1.5707963267948966;
            a.thickness = 0.5;

            let rt = roundtrip(py, EntityType::Arc(a)).unwrap();
            if let EntityType::Arc(b) = rt {
                assert_eq!(b.common.handle.value(), 9);
                assert_v3_eq(&b.center, &acadrust::types::Vector3::new(1.0, 2.0, 0.0));
                assert_f64_eq(b.radius, 5.0, "arc radius");
                assert_f64_eq(b.start_angle, 0.0, "arc start");
                assert_f64_eq(b.end_angle, 1.5707963267948966, "arc end");
                assert_f64_eq(b.thickness, 0.5, "arc thickness");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn ellipse_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut e = Ellipse::default();
            e.common.handle = Handle::new(10);
            e.center = acadrust::types::Vector3::new(10.0, 20.0, 0.0);
            e.major_axis = acadrust::types::Vector3::new(30.0, 0.0, 0.0);
            e.minor_axis_ratio = 0.5;
            e.start_parameter = 0.0;
            e.end_parameter = std::f64::consts::TAU;

            let rt = roundtrip(py, EntityType::Ellipse(e)).unwrap();
            if let EntityType::Ellipse(f) = rt {
                assert_eq!(f.common.handle.value(), 10);
                assert_v3_eq(&f.center, &acadrust::types::Vector3::new(10.0, 20.0, 0.0));
                assert_v3_eq(
                    &f.major_axis,
                    &acadrust::types::Vector3::new(30.0, 0.0, 0.0),
                );
                assert_f64_eq(f.minor_axis_ratio, 0.5, "ellipse ratio");
                assert_f64_eq(f.start_parameter, 0.0, "ellipse start");
                assert_f64_eq(f.end_parameter, std::f64::consts::TAU, "ellipse end");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn polyline_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut p = Polyline::default();
            p.common.handle = Handle::new(11);
            p.vertices = vec![
                acadrust::entities::Vertex3D::new(acadrust::types::Vector3::new(0.0, 0.0, 0.0)),
                acadrust::entities::Vertex3D::new(acadrust::types::Vector3::new(1.0, 0.0, 0.0)),
                acadrust::entities::Vertex3D::new(acadrust::types::Vector3::new(1.0, 1.0, 0.0)),
            ];
            // Note: Polyline stores closed state in flags, not a direct setter.
            // We only verify vertices and handle round-trip here.

            let rt = roundtrip(py, EntityType::Polyline(p)).unwrap();
            if let EntityType::Polyline(q) = rt {
                assert_eq!(q.common.handle.value(), 11);
                assert_eq!(q.vertices.len(), 3);
                assert_v3_eq(
                    &q.vertices[0].location,
                    &acadrust::types::Vector3::new(0.0, 0.0, 0.0),
                );
                assert_v3_eq(
                    &q.vertices[2].location,
                    &acadrust::types::Vector3::new(1.0, 1.0, 0.0),
                );
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn polyline2d_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut p = Polyline2D::default();
            p.common.handle = Handle::new(12);
            p.vertices = vec![
                acadrust::entities::Vertex2D::new(acadrust::types::Vector3::new(0.0, 0.0, 0.0)),
                acadrust::entities::Vertex2D::new(acadrust::types::Vector3::new(2.0, 0.0, 0.0)),
            ];
            p.elevation = 5.0;
            p.thickness = 1.0;

            let rt = roundtrip(py, EntityType::Polyline2D(p)).unwrap();
            if let EntityType::Polyline2D(q) = rt {
                assert_eq!(q.common.handle.value(), 12);
                assert_eq!(q.vertices.len(), 2);
                assert_f64_eq(q.elevation, 5.0, "polyline2d elevation");
                assert_f64_eq(q.thickness, 1.0, "polyline2d thickness");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn polyline3d_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut p = Polyline3D::default();
            p.common.handle = Handle::new(13);
            p.vertices = vec![
                acadrust::entities::Vertex3DPolyline::new(acadrust::types::Vector3::new(
                    0.0, 0.0, 0.0,
                )),
                acadrust::entities::Vertex3DPolyline::new(acadrust::types::Vector3::new(
                    0.0, 0.0, 3.0,
                )),
            ];
            p.flags.closed = true;

            let rt = roundtrip(py, EntityType::Polyline3D(p)).unwrap();
            if let EntityType::Polyline3D(q) = rt {
                assert_eq!(q.common.handle.value(), 13);
                assert_eq!(q.vertices.len(), 2);
                assert!(q.flags.closed);
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn lwpolyline_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut p = LwPolyline::new();
            p.common.handle = Handle::new(14);
            p.vertices = vec![
                LwVertex::new(acadrust::types::Vector2::new(0.0, 0.0)),
                LwVertex::new(acadrust::types::Vector2::new(4.0, 0.0)),
                LwVertex::new(acadrust::types::Vector2::new(4.0, 3.0)),
            ];
            p.is_closed = true;
            p.constant_width = 0.5;

            let rt = roundtrip(py, EntityType::LwPolyline(p)).unwrap();
            if let EntityType::LwPolyline(q) = rt {
                assert_eq!(q.common.handle.value(), 14);
                assert_eq!(q.vertices.len(), 3);
                assert!(q.is_closed);
                assert_f64_eq(q.constant_width, 0.5, "lwpolyline width");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn spline_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut s = Spline::default();
            s.common.handle = Handle::new(15);
            s.degree = 3;
            s.flags.closed = true;
            s.control_points = vec![
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
                acadrust::types::Vector3::new(1.0, 1.0, 0.0),
                acadrust::types::Vector3::new(2.0, 0.0, 0.0),
            ];
            s.knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
            s.weights = vec![1.0, 1.0, 1.0];

            let rt = roundtrip(py, EntityType::Spline(s)).unwrap();
            if let EntityType::Spline(t) = rt {
                assert_eq!(t.common.handle.value(), 15);
                assert_eq!(t.degree, 3);
                assert!(t.flags.closed);
                assert_eq!(t.control_points.len(), 3);
                assert_eq!(t.knots.len(), 6);
                assert_eq!(t.weights.len(), 3);
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn mtext_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut t = MText::default();
            t.common.handle = Handle::new(16);
            t.value = "hello".to_string();
            t.insertion_point = acadrust::types::Vector3::new(5.0, 5.0, 0.0);
            t.height = 2.5;

            let rt = roundtrip(py, EntityType::MText(t)).unwrap();
            if let EntityType::MText(u) = rt {
                assert_eq!(u.common.handle.value(), 16);
                assert_eq!(u.value, "hello");
                assert_v3_eq(
                    &u.insertion_point,
                    &acadrust::types::Vector3::new(5.0, 5.0, 0.0),
                );
                assert_f64_eq(u.height, 2.5, "mtext height");
            } else {
                panic!("wrong kind");
            }
        });
    }

    #[test]
    fn xdata_record_roundtrips() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let record = ExtendedDataRecord {
                application_name: "PYREPL".to_string(),
                values: vec![
                    XDataValue::String("hello".to_string()),
                    XDataValue::Real(3.14),
                    XDataValue::Integer32(42),
                    XDataValue::Point3D(acadrust::types::Vector3::new(1.0, 2.0, 3.0)),
                ],
            };

            let obj = xdata_record_to_py(py, &record).unwrap();
            let bound = obj.bind(py).downcast::<PyDict>().unwrap();
            let roundtripped = py_to_xdata_record(bound).unwrap();

            assert_eq!(roundtripped.application_name, "PYREPL");
            assert_eq!(roundtripped.values.len(), 4);
            assert_eq!(
                roundtripped.values[0],
                XDataValue::String("hello".to_string())
            );
            assert_eq!(roundtripped.values[1], XDataValue::Real(3.14));
            assert_eq!(roundtripped.values[2], XDataValue::Integer32(42));
            assert_eq!(
                roundtripped.values[3],
                XDataValue::Point3D(acadrust::types::Vector3::new(1.0, 2.0, 3.0))
            );
        });
    }
}
