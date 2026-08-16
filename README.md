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
ocs.doc.add({"kind": "Point", **p.__dict__})
```

Use `ocs.doc.refresh()` to re-read the snapshot after the host updates it.

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

### Add 1000 random points

```python
import random
import time
from ocs.entities import Point

random.seed(42)
points = [
    {"kind": "Point", **Point(
        x=random.uniform(-100.0, 100.0),
        y=random.uniform(-100.0, 100.0),
        z=0.0,
    ).__dict__}
    for _ in range(1000)
]
ocs.doc.add_many(points)

# The host drains plugin requests on a 5 ms timer; give it a moment, then refresh.
time.sleep(0.2)
ocs.doc.refresh()
print(ocs.doc.counts())
```

### Update and remove entities

`ocs.doc.update(entity_dict)` replaces the entity whose `handle` matches the
dictionary. `ocs.doc.remove(handle)` deletes it.

```python
from ocs.entities import Point, Circle

# Create two entities.
h1 = ocs.doc.add({"kind": "Point", **Point(x=10.0, y=10.0, z=0.0).__dict__})
h2 = ocs.doc.add({"kind": "Circle", **Circle(center=(50.0, 50.0, 0.0), radius=10.0).__dict__})

# Move the point (keep the same handle).
p = Point(x=30.0, y=30.0, z=0.0)
p.handle = h1
ocs.doc.update({"kind": "Point", **p.__dict__})

# Change the circle radius.
c = Circle(center=(50.0, 50.0, 0.0), radius=25.0)
c.handle = h2
ocs.doc.update({"kind": "Circle", **c.__dict__})

# Remove the point.
ocs.doc.remove(h1)

import time
time.sleep(0.2)
ocs.doc.refresh()
print(ocs.doc.counts())
```

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
    {"kind": "Point", **Point(
        x=random.uniform(-1000.0, 1000.0),
        y=random.uniform(-1000.0, 1000.0),
        z=0.0,
    ).__dict__}
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
- `py.typed` — PEP 561 marker for typing tools.

The compiled `_ocs` extension is **not** copied into the session dir. It is
loaded from the plugin directory via `PYTHONPATH` (the `ocs/__init__.py` fallback
imports `ocs_python_repl` as `_ocs`).

These directories are removed when the document tab closes or when
OpenCADStudio shuts down.
