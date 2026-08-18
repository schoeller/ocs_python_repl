# Python REPL Examples

These scripts demonstrate how to use the `ocs_python_repl` plugin from an
IPython terminal attached to OpenCADStudio.

## Running an example

1. Start OpenCADStudio and open a drawing tab.
2. Type `PYTHONSHELL` and press Enter to open the IPython terminal.
3. Inside IPython, import the example:

   ```python
   %pyimport assets/examples/python_repl/01_basic_entities.py
   ```

   The path is relative to the OpenCADStudio workspace root. If you run from a
different working directory, use the absolute path instead.

## Examples

| File | What it shows |
|---|---|
| `01_basic_entities.py` | Create Point, Line, Circle, Arc, Ellipse, Polyline, LwPolyline, Spline, and MText. |
| `02_modify_entities.py` | Update and remove entities by handle, including Polyline, Spline, and MText. |
| `03_batch_points.py` | Add thousands of points efficiently with `add_many`. |
| `04_query_entities.py` | Refresh the snapshot and query counts/entities/layers. |
| `05_advanced_entities.py` | Create Ellipse, Polyline2D, Polyline3D, and use Vector3/Color/Layer/XDataValue. |
| `06_random_points_100.py` | Add exactly 100 random points in a 100 x 100 area. |

## Supported entity operations

- **Create/update:** `ocs.doc.add(entity)`, `ocs.doc.add_many([...])`, and
  `ocs.doc.update(entity)` accept generated dataclass instances or plain
  dictionaries with a `kind` key. Supported kinds:

  `Point`, `Line`, `Circle`, `Arc`, `Ellipse`, `Polyline`, `Polyline2D`,
  `Polyline3D`, `LwPolyline`, `Spline`, `MText`.

- **Remove:** `ocs.doc.remove(handle)` works for any entity kind.

- **Read:** `ocs.doc.counts()`, `ocs.doc.entity(handle)`, `ocs.doc.entities()`,
  and `ocs.doc.layers()` can read all of the above entity kinds.

Support classes for `Vector2`, `Vector3`, `Color`, `Layer`, and `XDataValue`
are also available in `ocs.entities` for building entity dictionaries and
working with extended data.

After any mutation, wait briefly for the host's request drain timer and then
call `ocs.doc.refresh()` to see the changes in the snapshot.
