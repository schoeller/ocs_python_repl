# Generated from templates/startup.py by ocs_python_repl/build.rs
"""Bootstrap the IPython REPL with OCS document access and magics."""
import os
import re
import shlex
import sys

_SESSION_DIR = os.path.dirname(os.path.abspath(__file__))
if _SESSION_DIR not in sys.path:
    sys.path.insert(0, _SESSION_DIR)

import ocs

_snapshot_path = os.environ.get("OCS_V4_SNAPSHOT", "")
if _snapshot_path:
    ocs.doc = ocs._init(_snapshot_path)
else:
    # Running outside the host (e.g. in tests). Provide a dummy document so
    # the module still imports and the helper functions are available.
    class _NoDocument:
        def __getattr__(self, name):
            raise RuntimeError(
                "ocs.doc is not available: run PYTHONSHELL from OpenCADStudio "
                "or set OCS_V4_SNAPSHOT to a valid snapshot path."
            )

    ocs.doc = _NoDocument()


def _pyimport(path):
    """Run a Python script in the current namespace."""
    if not os.path.isfile(path):
        print(f"pyimport: file not found: {path}", file=sys.stderr)
        return
    with open(path, "r", encoding="utf-8") as f:
        code = f.read()
    exec(code, globals())


def _pyexport(path):
    """Write the current session's input history to a file."""
    content = "# No input history available in plain Python REPL.\n"
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)
    print(f"Exported session to {path}")


try:
    from IPython.core.magic import register_line_magic

    @register_line_magic
    def pyimport(line):
        path = line.strip().strip('"').strip("'")
        if not os.path.isfile(path):
            print(f"pyimport: file not found: {path}", file=sys.stderr)
            return
        get_ipython().run_line_magic("run", f"-i {shlex.quote(path)}")

    @register_line_magic
    def pyexport(line):
        path = line.strip().strip('"').strip("'")
        hist = list(get_ipython().history_manager.get_range())
        cells = []
        for _session, _line, cell in hist:
            cell = cell.strip()
            if cell.startswith("%pyexport"):
                continue
            # Strip IPython continuation prompts.
            cell = re.sub(r"^\s*\.{3,}\s?", "", cell, flags=re.MULTILINE)
            cells.append(cell)
        if cells:
            content = "\n".join(cells) + "\n"
        else:
            content = "# No input history to export.\n"
        with open(path, "w", encoding="utf-8") as f:
            f.write(content)
        print(f"Exported session to {path}")

except Exception:
    # Plain Python REPL fallback.
    def pyimport(path):
        _pyimport(path)

    def pyexport(path):
        _pyexport(path)
