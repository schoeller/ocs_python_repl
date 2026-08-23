# Python REPL Plugin

Out-of-process V4 plugin for Open CAD Studio that opens an IPython REPL in an
OS terminal window bound to the active document tab. Python code reads the live
document through a zero-copy shared-memory snapshot and mutates it through the
plugin API.

## Architecture

```text
Host (OpenCADStudio)
  └── PluginManager
        └── V4 runner process (ocs_python_repl.dll/.so/.dylib)
              └── PythonReplPlugin
                    └── ReplSession per tab_id
                          ├── Document -> SharedDocumentReader<DocumentViewDataV4>
                          ├── request-proxy TCP socket (PluginRequest -> host)
                          └── temp session dir (repl_wrapper.py, startup.py, ocs/)
                                └── Wrapper child: python repl_wrapper.py
                                      └── OS terminal: python -m IPython -i startup.py
```

Data flows:

- **Reads:** `_ocs` Rust extension → `SharedDocumentReader` → decode `EntityViewV4.data` as `bincode(EntityType)` → Python typed `Entity` objects.
- **Mutations:** Python `_ocs` → `ProxyPluginRequestSender` over `OCS_REQUEST_PORT` → host `AddEntity` / `UpdateEntity` / `RemoveEntity` / XDATA requests.
- **Refresh:** call `ocs.doc.refresh()` to re-map the snapshot after the host republishes it.
- **Import:** `%pyimport <path>` runs a script in the current IPython namespace.
- **Export:** `%pyexport <path>` writes the IPython input history to a file.

On host shutdown the V4 runner receives a graceful shutdown request, every active
session is dropped, and each session kills its wrapper child and deletes its temp
session dir.

## Binding generation

The Python entity classes, stubs, and Rust `entity_to_py` / `py_to_entity`
conversions are generated at **build time**, not at runtime. The
`ocs_plugin_api` embedded type registry
(`get_embedded_type_registry_json()`) is the single source of truth for
`acadrust` field types; `crud_manifest.json` is a type-filtered public-API
projection on top of that registry.

### Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ ocs_plugin_api build output                                                 │
│   OUT_DIR/type_registry.json  (serde-reflection of allow-listed acadrust   │
│                                types: Point, Line, Spline, EntityCommon, ...) │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │ read by ocs_python_repl/build.rs
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ crud_manifest.json                                                          │
│   type_filter  -> central allow-list of entity kinds exposed to Python      │
│   base_fields  -> EntityCommon fields promoted to base Entity class         │
│   overrides    -> constructors, renames, flattens, defaults, custom Rust      │
│   manual_overrides -> entities still requiring hand-written code              │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
              ┌────────────────┴────────────────┐
              ▼                                 ▼
┌─────────────────────────────┐     ┌─────────────────────────────────────────┐
│ build/generate.rs           │     │ OUT_DIR/python/ocs/                     │
│   Python dataclass generator│────▶│   entities.py                           │
│   Python stub generator     │     │   entities.pyi                          │
│   Rust CRUD generator       │────▶│ OUT_DIR/entity_crud.rs                  │
└─────────────────────────────┘     │   (included by src/python_ext.rs)       │
                                    └─────────────────────────────────────────┘
```

Cargo builds `ocs_plugin_api` first, so the registry JSON is already on disk
when `ocs_python_repl/build.rs` runs. The generator resolves each entity's
fields by combining the registry `TypeInfo` with any manifest override. Fields
not mentioned in `overrides` are emitted automatically with a registry-derived
Python type, default value, and Rust getter/setter.

### `crud_manifest.json` override mechanism

Only provide overrides when the public API must differ from the raw registry:

| Override | Purpose | Example |
|---|---|---|
| `constructor` | How to construct the Rust struct before field assignment. | `Point` uses `from_coords(x, y, z)`. |
| `python_name` | Expose a registry field under a different Python name. | `MText.value` is exposed as `text`. |
| `python_type` / `default` | Change the public Python type or default literal. | `Polyline.normal` defaults to `(0.0, 0.0, 1.0)`. |
| `flatten` | Expand a struct field into several Python fields. | `Spline.flags` becomes `closed`, `periodic`, `rational`, `planar`, `linear`. |
| `rust_getter` / `rust_setter` | Custom Rust expressions when the registry cannot express the conversion. | `Polyline.vertices` is exposed as `points: List[Tuple[float, float, float]]`. |
| `exclude` | Omit a registry field from the public API. | (rare; use only for internal-only data) |

#### Example overrides

**Point** — flatten `location` into `x`, `y`, `z` and use the `from_coords`
constructor:

```json
{
  "type_filter": ["Point"],
  "overrides": {
    "Point": {
      "constructor": { "kind": "from_coords", "args": ["x", "y", "z"] },
      "fields": {
        "location": { "flatten": ["x", "y", "z"], "default": "(0.0, 0.0, 0.0)" },
        "normal": { "default": "(0.0, 0.0, 1.0)" }
      }
    }
  }
}
```

**Polyline** — expose `vertices` as the user-friendly `points` list and expose
`flags` as the boolean `closed` field:

```json
{
  "overrides": {
    "Polyline": {
      "constructor": { "kind": "default" },
      "fields": {
        "vertices": {
          "python_name": "points",
          "python_type": "List[Tuple[float, float, float]]",
          "default": "[]",
          "rust_getter": "p.vertices.iter().map(|v| v3_tuple(&v.location)).collect::<Vec<_>>()",
          "rust_setter": "if let Some(pts) = entity_attr(entity, \"points\") { p.vertices = point_list(&pts)?.into_iter().map(acadrust::entities::Vertex3D::new).collect(); }"
        },
        "flags": {
          "python_name": "closed",
          "python_type": "bool",
          "default": "False",
          "rust_getter": "p.flags.is_closed()",
          "rust_setter": "p.flags.set_closed(get_opt_bool(entity, \"closed\", false));"
        }
      }
    }
  }
}
```

**Spline** — flatten the `SplineFlags` struct into individual boolean fields
with no custom Rust code:

```json
{
  "overrides": {
    "Spline": {
      "constructor": { "kind": "default" },
      "fields": {
        "flags": { "flatten": ["closed", "periodic", "rational", "planar", "linear"] }
      }
    }
  }
}
```

The generated files are copied into each session directory at runtime.

## Installation

The plugin is a standalone Rust workspace under `crates/ocs_python_repl`.

```powershell
cargo build --manifest-path crates/ocs_python_repl/Cargo.toml
```

Install the package into Open CAD Studio's plugin directory:

```powershell
$pluginDir = "$env:APPDATA\OpenCADStudio\plugins\opencad.python_repl"
New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
Copy-Item crates\ocs_python_repl\plugin.toml              $pluginDir
Copy-Item crates\ocs_python_repl\target\debug\ocs_python_repl.dll $pluginDir
```

> Note: the standalone workspace builds into `crates/ocs_python_repl/target/debug/`,
> not the root `target/debug/`.

### Requirements

- Python 3.8+ installed and on `PATH`.
- IPython (recommended for the best REPL experience):
  ```powershell
  python -m pip install ipython
  ```
  If IPython is missing the plugin falls back to a plain Python REPL.
- A terminal emulator on Linux/macOS. The wrapper tries, in order:
  `xterm`, `konsole`, `xfce4-terminal`, `gnome-terminal`, `alacritty`, `kitty`,
  `wezterm`. On macOS it also uses `Terminal.app` as a best-effort fallback.

### Debug host stack size

The OpenCADStudio debug binary needs a larger-than-default main thread stack on
Windows. After building the host, patch the PE header:

```powershell
editbin /STACK:16777216,65536 target/debug/OpenCADStudio.exe
```

## Usage

1. Start OpenCADStudio and open a drawing tab.
2. Type `PYTHONSHELL` in the command line and press Enter (or use the **Python
   Shell** ribbon button).
3. An OS terminal window opens with an IPython prompt connected to the active
   document.
4. Re-running `PYTHONSHELL` for the same tab is a no-op while the REPL is still
   alive.

```python
import ocs
from ocs.entities import Point

print(ocs.doc.counts())
p = Point(x=10, y=20, z=0)
ocs.doc.add(p)
```

`add`, `update`, and `add_many` accept either a generated dataclass instance or
an equivalent dict with a `kind` key. Use `ocs.doc.refresh()` to re-read the
snapshot after the host updates it.

## Examples

More example scripts are in [`assets/examples/python_repl`](assets/examples/python_repl):

- `01_basic_entities.py` — create Point, Line, Circle, Arc, Ellipse, Polyline,
  LwPolyline, Spline, and MText.
- `02_modify_entities.py` — update and remove entities by handle, including
  Polyline, Spline, and MText.
- `03_batch_points.py` — add thousands of points with `add_many`.
- `04_query_entities.py` — query counts, layers, and entity lists.
- `05_advanced_entities.py` — create Ellipse, Polyline2D, Polyline3D, and use
  Vector3, Color, Layer, and XDataValue.
- `06_random_points_100.py` — add exactly 100 random points in a 100 x 100 area.
- `07_osmnx_parks.py` — fetch OpenStreetMap park polygons with OSMnx and import
  them as closed LwPolylines on layer `_park`. Requires `osmnx`.

### Generate 100 random points

```python
import random
from ocs.entities import Point

random.seed(42)
points = [
    Point(
        x=random.uniform(-50.0, 50.0),
        y=random.uniform(-50.0, 50.0),
        z=0.0,
    )
    for _ in range(100)
]
handles = ocs.doc.add_many(points)
print(f"Added {len(handles)} points")
```

### Add 1000 random points

```python
import random
import time
from ocs.entities import Point

random.seed(42)
points = [
    Point(
        x=random.uniform(-100.0, 100.0),
        y=random.uniform(-100.0, 100.0),
        z=0.0,
    )
    for _ in range(1000)
]
ocs.doc.add_many(points)

# The host drains plugin requests on a 5 ms timer; give it a moment, then refresh.
time.sleep(0.2)
ocs.doc.refresh()
print(ocs.doc.counts())
```

### Update and remove entities

`ocs.doc.update(entity)` replaces the entity whose `handle` matches.
`ocs.doc.remove(handle)` deletes it.

```python
from ocs.entities import Point, Circle

# Create two entities.
h1 = ocs.doc.add(Point(x=10.0, y=10.0, z=0.0))
h2 = ocs.doc.add(Circle(center=(50.0, 50.0, 0.0), radius=10.0))

# Move the point (keep the same handle).
p = Point(x=30.0, y=30.0, z=0.0)
p.handle = h1
ocs.doc.update(p)

# Change the circle radius.
c = Circle(center=(50.0, 50.0, 0.0), radius=25.0)
c.handle = h2
ocs.doc.update(c)

# Remove the point.
ocs.doc.remove(h1)

import time
time.sleep(0.2)
ocs.doc.refresh()
print(ocs.doc.counts())
```

### Dict-based equivalent

For callers that prefer plain dictionaries, pass a dict with `kind` and the
field names from the generated dataclass:

```python
ocs.doc.add({"kind": "Point", "x": 10.0, "y": 10.0, "z": 0.0})
ocs.doc.update({"kind": "Point", "handle": h1, "x": 30.0, "y": 30.0, "z": 0.0})
```

When passing a dict, the `kind` key must match an entity class name (e.g.
`"Point"`, `"Circle"`). When passing a dataclass, the class name is used
automatically.

### Add 10000 random points with performance measurement

`ocs.doc.add_many(...)` sends the whole batch in one host request, so the wall
clock time is dominated by host-side entity insertion and a single snapshot
publish, not by 10000 TCP round-trips.

```python
import random
import time
from ocs.entities import Point

N = 10000
random.seed(42)
points = [
    Point(
        x=random.uniform(-1000.0, 1000.0),
        y=random.uniform(-1000.0, 1000.0),
        z=0.0,
    )
    for _ in range(N)
]

# Submit the entire batch in one request.
t0 = time.perf_counter()
handles = ocs.doc.add_many(points)
t1 = time.perf_counter()

elapsed = t1 - t0
print(f"Submitted {N} points in {elapsed:.3f}s ({N / elapsed:,.0f} points/s)")
print(f"Returned {len(handles)} handles, first = {handles[0]}")

# The host drains plugin requests on a 5 ms timer; wait briefly, then refresh.
time.sleep(0.3)
ocs.doc.refresh()
counts = ocs.doc.counts()
print(counts)
print(f"Total entities in document: {sum(counts.values())}")
```

Example output (debug build, Windows, will vary by scene complexity):

```text
Submitted 10000 points in 0.12s (83,333 points/s)
Returned 10000 handles, first = 98
{'Point': 10000}
Total entities in document: 10000
```

> **Why this is fast:** the Python side pays for one serialization + one socket
> round-trip; the host adds every entity inside a single `add_entities` call and
> republishes the shared snapshot once. Calling `ocs.doc.add` in a loop would
> spend most of its time waiting for the per-request response.

### Import / export scripts

Inside the IPython REPL:

```python
%pyimport C:\path\to\script.py
%pyexport C:\path\to\out.py
```

`%pyimport` executes the file in the current namespace. `%pyexport` writes the
IPython input history (excluding the `%pyexport` line itself, with IPython
continuation prompts stripped) to the file.

## Generated session files

Each `PYTHONSHELL` session creates a temp directory such as
`%TEMP%\ocs_python_repl_<pid>_<tab_id>_<timestamp>\` containing:

- `repl_wrapper.py` — platform wrapper that launches the terminal/IPython.
- `startup.py` — bootstraps `ocs.doc`, registers `%pyimport` / `%pyexport`.
- `ocs/` — Python package with `_ocs` extension wrapper, entity classes, and stubs.
  `ocs/entities.py` and `ocs/entities.pyi` are generated at build time.
- `py.typed` — PEP 561 marker for typing tools.

The compiled `_ocs` extension is **not** copied into the session dir. It is
loaded from the plugin directory via `PYTHONPATH` (the `ocs/__init__.py` fallback
imports `ocs_python_repl` as `_ocs`).

These directories are removed when the document tab closes or when
OpenCADStudio shuts down.

## Adding or customizing entity bindings

The registry in `ocs_plugin_api` is the single source of truth for field types.
The Python/Rust binding for each entity kind is generated from `crud_manifest.json`
in this crate. To add a new entity kind or override an existing one:

1. Make sure the type is traced by the `ocs_plugin_api` allow-list in
   `crates/ocs_plugin_api/build.rs` (add an `("MyEntity", trace::<acadrust::MyEntity>)`
   entry). If the entity contains enums that `serde-reflection` has not seen,
   add sample values in `add_enum_samples`.

2. Add the entity to `crud_manifest.json` under `type_filter` and, only if the
   public API needs to differ from the registry, add an entry under `overrides`:

   ```json
   {
     "type_filter": ["Point", "MyEntity"],
     "overrides": {
       "MyEntity": {
         "constructor": { "kind": "new" },
         "fields": {
           "center": { "default": "(0.0, 0.0, 0.0)" }
         }
       }
     }
   }
   ```

   The generator derives each field's Python type and Rust getter/setter from
   the registry. Only supply overrides for:

   - `constructor` — `new`, `default`, or `from_coords` (with `args`).
   - `python_name` — expose a registry field under a different Python name.
   - `python_type` / `default` — change the public type or default literal.
   - `flatten` — expand a struct field (e.g. `SplineFlags`) into individual
     Python fields.
   - `rust_getter` / `rust_setter` — custom Rust expressions for fields where
     the registry cannot express the conversion (e.g. bitflags, vertex lists
     exposed as plain point tuples).

3. If the entity cannot be fully described by the manifest/registry and needs
   hand-written Rust or Python code, add an entry to the `manual_overrides`
   section explaining why and how to maintain it, and add the required helpers
   to `src/python_ext.rs`:

   ```json
   "manual_overrides": {
     "MyEntity": "MyEntity uses a custom bitflags struct for style flags that is not yet in the registry. Provide a custom rust_setter for the style field and a helper in python_ext.rs until the registry covers it."
   }
   ```

4. Rebuild the plugin and run the tests. Round-trip tests for every entity kind
   are in `python_ext.rs`; add a new test case following the existing pattern if
   you want to assert entity-specific invariants.

   ```powershell
   cargo test -p ocs_python_repl --manifest-path crates/ocs_python_repl/Cargo.toml
   ```
