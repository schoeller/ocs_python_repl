# Open CAD Studio Python REPL package.
# The compiled Rust extension is loaded as `ocs_python_repl` and re-exported
# under the `ocs` namespace. Generated entity classes live in `ocs.entities`.
#
# Implementation note: we replace `sys.modules[__name__]` with the extension
# module object so that `import ocs` gives users the Rust-backed `_OcsDocument`
# directly. We then set `__path__` so that `ocs.entities` still resolves to the
# generated submodule for editor tooling and type checkers.

import os
import shutil
import sys

_package_dir = os.path.dirname(os.path.abspath(__file__))
_parent_dir = os.path.dirname(_package_dir)
if _parent_dir not in sys.path:
    sys.path.insert(0, _parent_dir)

def _load_extension():
    """Load the Rust extension, coping with platform-specific import suffixes."""
    try:
        import ocs_python_repl as _ocs  # type: ignore
        return _ocs
    except ImportError:
        pass

    _cdylib_path = os.environ.get("OCS_PLUGIN_CDYLIB_PATH", "")
    if not _cdylib_path or not os.path.isfile(_cdylib_path):
        raise ImportError(
            "The OCS Python extension (ocs_python_repl) is not available. "
            "Make sure the Python REPL plugin is loaded and the plugin directory "
            "is on PYTHONPATH."
        )

    # Python's import machinery requires the file name to match the module
    # name. On Windows the extension suffix is .pyd; on Linux/macOS it is .so.
    # The installed cdylib has a platform-specific release name, so create a
    # hardlink/copy with the expected import name in the session directory.
    if sys.platform == "win32":
        _ext_path = os.path.join(_parent_dir, "ocs_python_repl.pyd")
    else:
        _ext_path = os.path.join(_parent_dir, "ocs_python_repl.so")
    if not os.path.isfile(_ext_path):
        try:
            os.link(_cdylib_path, _ext_path)
        except Exception:
            shutil.copy2(_cdylib_path, _ext_path)
    import ocs_python_repl as _ocs  # type: ignore
    return _ocs

_ocs = _load_extension()

sys.modules[__name__] = _ocs

# Keep the package path resolvable for editor tooling.
if not hasattr(_ocs, "__path__"):
    _ocs.__path__ = [_package_dir]
