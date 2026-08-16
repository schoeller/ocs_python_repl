"""Open CAD Studio Python REPL package.

The compiled Rust extension is loaded as `_ocs` and re-exported under the
`ocs` namespace. Generated entity classes live in `ocs.entities`.
"""

import sys
import os

_package_dir = os.path.dirname(os.path.abspath(__file__))
_parent_dir = os.path.dirname(_package_dir)
if _parent_dir not in sys.path:
    sys.path.insert(0, _parent_dir)

try:
    import _ocs
except ImportError:
    # Fallback: when running inside the editor, the extension may be named
    # after the crate directory.
    import ocs_python_repl as _ocs  # type: ignore

sys.modules[__name__] = _ocs

# Keep the package path resolvable for editor tooling.
if not hasattr(_ocs, "__path__"):
    _ocs.__path__ = [_package_dir]
