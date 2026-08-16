//! PyO3 `_ocs` extension module: document binding and entity conversion.

use acadrust::entities::{
    Arc as CadArc, Circle, Ellipse, Entity as EntityTrait, Line, LwPolyline, LwVertex, MText,
    Point, Polyline, Polyline2D, Polyline3D, Spline, SplineFlags,
};
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::EntityType;
use pyo3::conversion::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

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
    m.add_class::<Document>()?;
    m.add_wrapped(wrap_pyfunction!(_init))?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (snapshot_path))]
fn _init(snapshot_path: String) -> PyResult<Document> {
    Document::new(snapshot_path)
}

// ═══════════════════════════════════════════════════════════════════════════
// Entity -> Python
// ═══════════════════════════════════════════════════════════════════════════

/// Convert an `acadrust::EntityType` into a Python `ocs.entities` dataclass.
pub fn entity_to_py(py: Python<'_>, entity: &EntityType, handle: u64) -> PyResult<PyObject> {
    let entities = py.import("ocs.entities")?;
    let (kind, kwargs): (&str, Bound<'_, PyDict>) = match entity {
        EntityType::Point(p) => ("Point", point_kwargs(py, p)?),
        EntityType::Line(l) => ("Line", line_kwargs(py, l)?),
        EntityType::Circle(c) => ("Circle", circle_kwargs(py, c)?),
        EntityType::Arc(a) => ("Arc", arc_kwargs(py, a)?),
        EntityType::Ellipse(e) => ("Ellipse", ellipse_kwargs(py, e)?),
        EntityType::Polyline(p) => ("Polyline", polyline_kwargs(py, p)?),
        EntityType::Polyline2D(p) => ("Polyline2D", polyline2d_kwargs(py, p)?),
        EntityType::Polyline3D(p) => ("Polyline3D", polyline3d_kwargs(py, p)?),
        EntityType::LwPolyline(p) => ("LwPolyline", lwpolyline_kwargs(py, p)?),
        EntityType::Spline(s) => ("Spline", spline_kwargs(py, s)?),
        EntityType::MText(t) => ("MText", mtext_kwargs(py, t)?),
        other => ("Entity", base_kwargs(py, other.common().layer.clone())?),
    };
    let cls = entities.getattr(kind)?;
    let obj = cls.call((), Some(&kwargs))?;
    obj.setattr("handle", handle)?;
    Ok(obj.into())
}

fn base_kwargs(py: Python<'_>, layer: String) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("layer", layer)?;
    Ok(dict)
}

fn v3_tuple(v: &acadrust::types::Vector3) -> (f64, f64, f64) {
    (v.x, v.y, v.z)
}

fn point_kwargs<'py>(py: Python<'py>, p: &Point) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, p.common.layer.clone())?;
    dict.set_item("x", p.location.x)?;
    dict.set_item("y", p.location.y)?;
    dict.set_item("z", p.location.z)?;
    dict.set_item("thickness", p.thickness)?;
    dict.set_item("normal", v3_tuple(&p.normal))?;
    dict.set_item("x_axis_angle", p.x_axis_angle)?;
    Ok(dict)
}

fn line_kwargs<'py>(py: Python<'py>, l: &Line) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, l.common.layer.clone())?;
    dict.set_item("start", v3_tuple(&l.start))?;
    dict.set_item("end", v3_tuple(&l.end))?;
    dict.set_item("thickness", l.thickness)?;
    dict.set_item("normal", v3_tuple(&l.normal))?;
    Ok(dict)
}

fn circle_kwargs<'py>(py: Python<'py>, c: &Circle) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, c.common.layer.clone())?;
    dict.set_item("center", v3_tuple(&c.center))?;
    dict.set_item("radius", c.radius)?;
    dict.set_item("thickness", c.thickness)?;
    dict.set_item("normal", v3_tuple(&c.normal))?;
    Ok(dict)
}

fn arc_kwargs<'py>(py: Python<'py>, a: &CadArc) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, a.common.layer.clone())?;
    dict.set_item("center", v3_tuple(&a.center))?;
    dict.set_item("radius", a.radius)?;
    dict.set_item("start_angle", a.start_angle)?;
    dict.set_item("end_angle", a.end_angle)?;
    dict.set_item("thickness", a.thickness)?;
    dict.set_item("normal", v3_tuple(&a.normal))?;
    Ok(dict)
}

fn ellipse_kwargs<'py>(py: Python<'py>, e: &Ellipse) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, e.common.layer.clone())?;
    dict.set_item("center", v3_tuple(&e.center))?;
    dict.set_item("major_axis", v3_tuple(&e.major_axis))?;
    dict.set_item("minor_axis_ratio", e.minor_axis_ratio)?;
    dict.set_item("start_parameter", e.start_parameter)?;
    dict.set_item("end_parameter", e.end_parameter)?;
    dict.set_item("normal", v3_tuple(&e.normal))?;
    Ok(dict)
}

fn polyline_kwargs<'py>(py: Python<'py>, p: &Polyline) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, p.common.layer.clone())?;
    let pts: Vec<(f64, f64, f64)> = p.vertices.iter().map(|v| v3_tuple(&v.location)).collect();
    dict.set_item("points", pts)?;
    dict.set_item("closed", p.is_closed())?;
    Ok(dict)
}

fn polyline2d_kwargs<'py>(py: Python<'py>, p: &Polyline2D) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, p.common.layer.clone())?;
    let pts: Vec<(f64, f64, f64)> = p.vertices.iter().map(|v| v3_tuple(&v.location)).collect();
    dict.set_item("points", pts)?;
    dict.set_item("closed", p.is_closed())?;
    dict.set_item("smooth_surface", smooth_surface_name(&p.smooth_surface))?;
    dict.set_item("start_width", p.start_width)?;
    dict.set_item("end_width", p.end_width)?;
    dict.set_item("thickness", p.thickness)?;
    dict.set_item("elevation", p.elevation)?;
    dict.set_item("normal", v3_tuple(&p.normal))?;
    Ok(dict)
}

fn polyline3d_kwargs<'py>(py: Python<'py>, p: &Polyline3D) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, p.common.layer.clone())?;
    let pts: Vec<(f64, f64, f64)> = p.vertices.iter().map(|v| v3_tuple(&v.position)).collect();
    dict.set_item("points", pts)?;
    dict.set_item("closed", p.flags.closed)?;
    dict.set_item("spline_fit", p.flags.spline_fit)?;
    dict.set_item("is_3d_mesh", p.flags.is_3d_mesh)?;
    dict.set_item("mesh_closed_n", p.flags.mesh_closed_n)?;
    dict.set_item("is_polyface_mesh", p.flags.is_polyface_mesh)?;
    dict.set_item("linetype_continuous", p.flags.linetype_continuous)?;
    dict.set_item("smooth_type", smooth_surface_name_3d(&p.smooth_type))?;
    dict.set_item("default_start_width", p.default_start_width)?;
    dict.set_item("default_end_width", p.default_end_width)?;
    dict.set_item("mesh_m_count", p.mesh_m_count)?;
    dict.set_item("mesh_n_count", p.mesh_n_count)?;
    dict.set_item("smooth_m_density", p.smooth_m_density)?;
    dict.set_item("smooth_n_density", p.smooth_n_density)?;
    dict.set_item("elevation", p.elevation)?;
    dict.set_item("normal", v3_tuple(&p.normal))?;
    Ok(dict)
}

fn lwpolyline_kwargs<'py>(py: Python<'py>, p: &LwPolyline) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, p.common.layer.clone())?;
    let pts: Vec<(f64, f64, f64)> = p
        .vertices
        .iter()
        .map(|v| (v.location.x, v.location.y, 0.0))
        .collect();
    dict.set_item("points", pts)?;
    dict.set_item("closed", p.is_closed)?;
    dict.set_item("plinegen", p.plinegen)?;
    dict.set_item("constant_width", p.constant_width)?;
    dict.set_item("elevation", p.elevation)?;
    dict.set_item("thickness", p.thickness)?;
    dict.set_item("normal", v3_tuple(&p.normal))?;
    Ok(dict)
}

fn spline_kwargs<'py>(py: Python<'py>, s: &Spline) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, s.common.layer.clone())?;
    dict.set_item("degree", s.degree)?;
    dict.set_item("closed", s.flags.closed)?;
    dict.set_item("periodic", s.flags.periodic)?;
    dict.set_item("rational", s.flags.rational)?;
    dict.set_item("planar", s.flags.planar)?;
    dict.set_item("linear", s.flags.linear)?;
    dict.set_item("knots", s.knots.clone())?;
    let pts: Vec<(f64, f64, f64)> = s.control_points.iter().map(v3_tuple).collect();
    dict.set_item("control_points", pts)?;
    dict.set_item("weights", s.weights.clone())?;
    let fit: Vec<(f64, f64, f64)> = s.fit_points.iter().map(v3_tuple).collect();
    dict.set_item("fit_points", fit)?;
    dict.set_item("normal", v3_tuple(&s.normal))?;
    dict.set_item("knot_tolerance", s.knot_tolerance)?;
    dict.set_item("control_tolerance", s.control_tolerance)?;
    dict.set_item("fit_tolerance", s.fit_tolerance)?;
    dict.set_item("begin_tangent", v3_tuple(&s.begin_tangent))?;
    dict.set_item("end_tangent", v3_tuple(&s.end_tangent))?;
    dict.set_item("knot_parameterization", s.knot_parameterization)?;
    dict.set_item("cv_frame_visible", s.cv_frame_visible)?;
    dict.set_item("dwg_flags1", s.dwg_flags1)?;
    Ok(dict)
}

fn mtext_kwargs<'py>(py: Python<'py>, t: &MText) -> PyResult<Bound<'py, PyDict>> {
    let dict = base_kwargs(py, t.common.layer.clone())?;
    dict.set_item("text", t.value.clone())?;
    dict.set_item("insertion", v3_tuple(&t.insertion_point))?;
    dict.set_item("height", t.height)?;
    Ok(dict)
}

fn smooth_surface_name(t: &acadrust::entities::SmoothSurfaceType) -> &'static str {
    match t {
        acadrust::entities::SmoothSurfaceType::None => "None",
        acadrust::entities::SmoothSurfaceType::QuadraticBSpline => "QuadraticBSpline",
        acadrust::entities::SmoothSurfaceType::CubicBSpline => "CubicBSpline",
        acadrust::entities::SmoothSurfaceType::Bezier => "Bezier",
    }
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
// Python -> Entity
// ═══════════════════════════════════════════════════════════════════════════

fn get_opt_string(entity: &Bound<'_, PyDict>, key: &str) -> String {
    let Some(value) = entity.get_item(key).ok().flatten() else {
        return String::new();
    };
    match value.extract::<String>() {
        Ok(s) => s,
        Err(_) => {
            warn_type_mismatch(&value, key, "string");
            String::new()
        }
    }
}

fn get_opt_f64(entity: &Bound<'_, PyDict>, key: &str, default: f64) -> f64 {
    let Some(value) = entity.get_item(key).ok().flatten() else {
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

fn get_opt_u64(entity: &Bound<'_, PyDict>, key: &str, default: u64) -> u64 {
    let Some(value) = entity.get_item(key).ok().flatten() else {
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

fn get_opt_i32(entity: &Bound<'_, PyDict>, key: &str, default: i32) -> i32 {
    let Some(value) = entity.get_item(key).ok().flatten() else {
        return default;
    };
    match value.extract::<i32>() {
        Ok(v) => v,
        Err(_) => {
            warn_type_mismatch(&value, key, "int");
            default
        }
    }
}

fn get_opt_bool(entity: &Bound<'_, PyDict>, key: &str, default: bool) -> bool {
    let Some(value) = entity.get_item(key).ok().flatten() else {
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
    entity: &Bound<'_, PyDict>,
    key: &str,
    default: acadrust::types::Vector3,
) -> acadrust::types::Vector3 {
    let Some(value) = entity.get_item(key).ok().flatten() else {
        return default;
    };
    match py_vector3(&value) {
        Ok(v) => v,
        Err(_) => {
            warn_type_mismatch(&value, key, "Vector3 tuple or object with x/y/z");
            default
        }
    }
}

fn smooth_surface_type(
    entity: &Bound<'_, PyDict>,
    key: &str,
) -> acadrust::entities::SmoothSurfaceType {
    match get_opt_string(entity, key).as_str() {
        "QuadraticBSpline" => acadrust::entities::SmoothSurfaceType::QuadraticBSpline,
        "CubicBSpline" => acadrust::entities::SmoothSurfaceType::CubicBSpline,
        "Bezier" => acadrust::entities::SmoothSurfaceType::Bezier,
        _ => acadrust::entities::SmoothSurfaceType::None,
    }
}

fn smooth_surface_type_3d(
    entity: &Bound<'_, PyDict>,
    key: &str,
) -> acadrust::entities::polyline3d::SmoothSurfaceType {
    match get_opt_string(entity, key).as_str() {
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

fn f64_list(entity: &Bound<'_, PyDict>, key: &str) -> Vec<f64> {
    let Some(value) = entity.get_item(key).ok().flatten() else {
        return Vec::new();
    };
    let Ok(list) = value.downcast::<PyList>() else {
        warn_type_mismatch(&value, key, "list of floats");
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| match item.extract::<f64>() {
            Ok(v) => Some(v),
            Err(_) => {
                warn_type_mismatch(&item, key, "float");
                None
            }
        })
        .collect()
}

fn set_common<T: EntityTrait>(e: &mut T, handle: u64, layer: String) {
    e.set_handle(acadrust::Handle::new(handle));
    e.set_layer(layer);
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

/// Convert a Python entity dict back into an `acadrust::EntityType`.
pub fn py_to_entity(entity: &Bound<'_, PyDict>) -> PyResult<EntityType> {
    let kind: String = entity
        .get_item("kind")?
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "entity dict missing 'kind'; supported kinds: Point, Line, Circle, Arc, Ellipse, \
                 Polyline, Polyline2D, Polyline3D, LwPolyline, Spline, MText"
                    .to_string(),
            )
        })?
        .extract()?;
    let layer = get_opt_string(entity, "layer");
    let handle = get_opt_u64(entity, "handle", 0);

    match kind.as_str() {
        "Point" => {
            let mut p = Point::from_coords(
                get_opt_f64(entity, "x", 0.0),
                get_opt_f64(entity, "y", 0.0),
                get_opt_f64(entity, "z", 0.0),
            );
            set_common(&mut p, handle, layer);
            p.thickness = get_opt_f64(entity, "thickness", 0.0);
            p.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            p.x_axis_angle = get_opt_f64(entity, "x_axis_angle", 0.0);
            Ok(EntityType::Point(p))
        }
        "Line" => {
            let mut l = Line::new();
            set_common(&mut l, handle, layer);
            l.start = py_vector3_opt(
                entity,
                "start",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            l.end = py_vector3_opt(entity, "end", acadrust::types::Vector3::new(0.0, 0.0, 0.0));
            l.thickness = get_opt_f64(entity, "thickness", 0.0);
            l.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            Ok(EntityType::Line(l))
        }
        "Circle" => {
            let mut c = Circle::new();
            set_common(&mut c, handle, layer);
            c.center = py_vector3_opt(
                entity,
                "center",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            c.radius = get_opt_f64(entity, "radius", 1.0);
            c.thickness = get_opt_f64(entity, "thickness", 0.0);
            c.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            Ok(EntityType::Circle(c))
        }
        "Arc" => {
            let mut a = CadArc::new();
            set_common(&mut a, handle, layer);
            a.center = py_vector3_opt(
                entity,
                "center",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            a.radius = get_opt_f64(entity, "radius", 1.0);
            a.start_angle = get_opt_f64(entity, "start_angle", 0.0);
            a.end_angle = get_opt_f64(entity, "end_angle", 0.0);
            a.thickness = get_opt_f64(entity, "thickness", 0.0);
            a.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            Ok(EntityType::Arc(a))
        }
        "Ellipse" => {
            let mut e = Ellipse::default();
            set_common(&mut e, handle, layer);
            e.center = py_vector3_opt(
                entity,
                "center",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            e.major_axis = py_vector3_opt(
                entity,
                "major_axis",
                acadrust::types::Vector3::new(1.0, 0.0, 0.0),
            );
            e.minor_axis_ratio = get_opt_f64(entity, "minor_axis_ratio", 0.5);
            e.start_parameter = get_opt_f64(entity, "start_parameter", 0.0);
            e.end_parameter = get_opt_f64(entity, "end_parameter", std::f64::consts::TAU);
            e.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            Ok(EntityType::Ellipse(e))
        }
        "Polyline" => {
            let mut p = Polyline::default();
            set_common(&mut p, handle, layer);
            if let Some(pts) = entity.get_item("points").ok().flatten() {
                let pts = point_list(&pts)?;
                p.vertices = pts
                    .into_iter()
                    .map(acadrust::entities::Vertex3D::new)
                    .collect();
            }
            Ok(EntityType::Polyline(p))
        }
        "Polyline2D" => {
            let mut p = Polyline2D::default();
            set_common(&mut p, handle, layer);
            p.smooth_surface = smooth_surface_type(entity, "smooth_surface");
            p.start_width = get_opt_f64(entity, "start_width", 0.0);
            p.end_width = get_opt_f64(entity, "end_width", 0.0);
            p.thickness = get_opt_f64(entity, "thickness", 0.0);
            p.elevation = get_opt_f64(entity, "elevation", 0.0);
            p.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            if let Some(pts) = entity.get_item("points").ok().flatten() {
                let pts = point_list(&pts)?;
                p.vertices = pts
                    .into_iter()
                    .map(acadrust::entities::Vertex2D::new)
                    .collect();
            }
            Ok(EntityType::Polyline2D(p))
        }
        "Polyline3D" => {
            let mut p = Polyline3D::default();
            set_common(&mut p, handle, layer);
            p.flags.closed = get_opt_bool(entity, "closed", false);
            p.flags.spline_fit = get_opt_bool(entity, "spline_fit", false);
            p.flags.is_3d_mesh = get_opt_bool(entity, "is_3d_mesh", false);
            p.flags.mesh_closed_n = get_opt_bool(entity, "mesh_closed_n", false);
            p.flags.is_polyface_mesh = get_opt_bool(entity, "is_polyface_mesh", false);
            p.flags.linetype_continuous = get_opt_bool(entity, "linetype_continuous", false);
            p.smooth_type = smooth_surface_type_3d(entity, "smooth_type");
            p.default_start_width = get_opt_f64(entity, "default_start_width", 0.0);
            p.default_end_width = get_opt_f64(entity, "default_end_width", 0.0);
            p.mesh_m_count = get_opt_u64(entity, "mesh_m_count", 0) as u16;
            p.mesh_n_count = get_opt_u64(entity, "mesh_n_count", 0) as u16;
            p.smooth_m_density = get_opt_u64(entity, "smooth_m_density", 0) as u16;
            p.smooth_n_density = get_opt_u64(entity, "smooth_n_density", 0) as u16;
            p.elevation = get_opt_f64(entity, "elevation", 0.0);
            p.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            if let Some(pts) = entity.get_item("points").ok().flatten() {
                let pts = point_list(&pts)?;
                p.vertices = pts
                    .into_iter()
                    .map(acadrust::entities::Vertex3DPolyline::new)
                    .collect();
            }
            Ok(EntityType::Polyline3D(p))
        }
        "LwPolyline" => {
            let mut p = LwPolyline::default();
            set_common(&mut p, handle, layer);
            p.is_closed = get_opt_bool(entity, "closed", false);
            p.plinegen = get_opt_bool(entity, "plinegen", false);
            p.constant_width = get_opt_f64(entity, "constant_width", 0.0);
            p.elevation = get_opt_f64(entity, "elevation", 0.0);
            p.thickness = get_opt_f64(entity, "thickness", 0.0);
            p.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            if let Some(pts) = entity.get_item("points").ok().flatten() {
                p.vertices = point_list(&pts)?
                    .into_iter()
                    .map(|v| LwVertex::new(acadrust::types::Vector2::new(v.x, v.y)))
                    .collect();
            }
            Ok(EntityType::LwPolyline(p))
        }
        "Spline" => {
            let mut s = Spline::default();
            set_common(&mut s, handle, layer);
            s.degree = get_opt_i32(entity, "degree", 3);
            s.flags = SplineFlags {
                closed: get_opt_bool(entity, "closed", false),
                periodic: get_opt_bool(entity, "periodic", false),
                rational: get_opt_bool(entity, "rational", false),
                planar: get_opt_bool(entity, "planar", false),
                linear: get_opt_bool(entity, "linear", false),
            };
            s.knots = f64_list(entity, "knots");
            s.control_points = entity
                .get_item("control_points")
                .ok()
                .flatten()
                .map(|obj| point_list(&obj))
                .transpose()?
                .unwrap_or_default();
            s.weights = f64_list(entity, "weights");
            s.fit_points = entity
                .get_item("fit_points")
                .ok()
                .flatten()
                .map(|obj| point_list(&obj))
                .transpose()?
                .unwrap_or_default();
            s.normal = py_vector3_opt(
                entity,
                "normal",
                acadrust::types::Vector3::new(0.0, 0.0, 1.0),
            );
            s.knot_tolerance = get_opt_f64(entity, "knot_tolerance", 1e-7);
            s.control_tolerance = get_opt_f64(entity, "control_tolerance", 1e-7);
            s.fit_tolerance = get_opt_f64(entity, "fit_tolerance", 1e-10);
            s.begin_tangent = py_vector3_opt(
                entity,
                "begin_tangent",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            s.end_tangent = py_vector3_opt(
                entity,
                "end_tangent",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            s.knot_parameterization = get_opt_i32(entity, "knot_parameterization", 0);
            s.cv_frame_visible = get_opt_bool(entity, "cv_frame_visible", false);
            s.dwg_flags1 = get_opt_i32(entity, "dwg_flags1", 0);
            Ok(EntityType::Spline(s))
        }
        "MText" => {
            let mut t = MText::default();
            set_common(&mut t, handle, layer);
            t.value = get_opt_string(entity, "text");
            t.insertion_point = py_vector3_opt(
                entity,
                "insertion",
                acadrust::types::Vector3::new(0.0, 0.0, 0.0),
            );
            t.height = get_opt_f64(entity, "height", 1.0);
            Ok(EntityType::MText(t))
        }
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "unsupported entity kind: {kind}"
        ))),
    }
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;
    use acadrust::Handle;

    fn py_kind_name(entity: &EntityType) -> &'static str {
        match entity {
            EntityType::Point(_) => "Point",
            EntityType::Line(_) => "Line",
            EntityType::Circle(_) => "Circle",
            EntityType::Arc(_) => "Arc",
            EntityType::Ellipse(_) => "Ellipse",
            EntityType::Polyline(_) => "Polyline",
            EntityType::Polyline2D(_) => "Polyline2D",
            EntityType::Polyline3D(_) => "Polyline3D",
            EntityType::LwPolyline(_) => "LwPolyline",
            EntityType::Spline(_) => "Spline",
            EntityType::MText(_) => "MText",
            _ => "Entity",
        }
    }

    fn roundtrip(py: Python<'_>, entity: EntityType) -> PyResult<EntityType> {
        // Load the generated `ocs/entities.py` dataclasses without importing the
        // full `ocs` package (which requires the compiled Rust extension).
        let python_dir = std::path::PathBuf::from(std::env!("OUT_DIR")).join("python");
        let entities_path = python_dir.join("ocs/entities.py");
        let code =
            std::fs::read_to_string(&entities_path).expect("generated entities.py should exist");
        #[allow(deprecated)]
        let entities_mod = PyModule::from_code_bound(py, &code, "entities.py", "ocs.entities")?;
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
        let dict: Bound<'_, PyDict> = bound.getattr("__dict__")?.downcast_into()?;
        dict.set_item("kind", py_kind_name(&entity))?;
        py_to_entity(&dict)
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
