"""
Fetch leisure=park polygons from OpenStreetMap and draw them as closed
LwPolylines on layer "_park".

Run inside OpenCADStudio after typing PYTHONSHELL:

    %pyimport assets/examples/python_repl/07_osmnx_parks.py

Requires: osmnx (which pulls geopandas and shapely)
    python -m pip install osmnx

After running, verify an attached OSM name with:

    ocs.doc.read_record(handle, "PYREPL")
"""
import time

# ---------------------------------------------------------------------------
# User-configurable defaults. Edit these values before %pyimport if desired.
# ---------------------------------------------------------------------------
LOCATION = "Frankfurt/Main"
PARK_LAYER = "_park"

# Set to a distance in meters (e.g. 1.0) to simplify dense OSM boundaries.
# None disables simplification and keeps the raw OSM geometry.
SIMPLIFY_TOLERANCE = None

# OSM tag filter: fetch only leisure=park features.
# "name": True ensures the OSM name tag is downloaded for XDATA.
PARK_TAGS = {
    "name": True,
    "leisure": "park",
}

# XDATA application name registered by ocs_python_repl (see plugin.toml).
XDATA_APP = "PYREPL"

# ---------------------------------------------------------------------------
print(f"Fetching parks for: {LOCATION}")

try:
    import osmnx as ox
except ImportError as exc:
    raise RuntimeError(
        "osmnx is required. Install it with: python -m pip install osmnx"
    ) from exc

# OSMnx >= 2.0 uses features_from_place; older versions used geometries_from_place.
if hasattr(ox, "features_from_place"):
    features_from_place = ox.features_from_place
else:
    features_from_place = ox.geometries_from_place

gdf = features_from_place(LOCATION, tags=PARK_TAGS)
print(f"Fetched {len(gdf)} OSM features")

from ocs.entities import LwPolyline


def _extract_name(row):
    """Return a clean OSM name string, or None if the feature is unnamed."""
    raw = row.get("name")
    if raw is None:
        return None
    # geopandas/pandas may return float('nan') for missing strings.
    if isinstance(raw, float):
        return None
    name = str(raw).strip()
    if not name:
        return None
    # DWG XDATA strings are typically limited to 255 bytes; truncate safely.
    return name[:255]


polylines = []
polyline_names = []
for _idx, row in gdf.iterrows():
    geom = row.geometry
    if geom is None:
        continue

    # Collect exterior rings from Polygon and MultiPolygon geometries.
    rings = []
    if geom.geom_type == "Polygon":
        rings.append(geom.exterior)
    elif geom.geom_type == "MultiPolygon":
        for part in geom.geoms:
            rings.append(part.exterior)
    else:
        # Skip LineString, Point, etc.
        continue

    name = _extract_name(row)
    for ring in rings:
        if SIMPLIFY_TOLERANCE is not None:
            ring = ring.simplify(SIMPLIFY_TOLERANCE, preserve_topology=True)

        # OSMnx returns projected coordinates in meters.
        coords = list(ring.coords)
        # Shapely exterior rings are already closed (last == first).
        # LwPolyline expects 2D points.
        points = [(float(x), float(y)) for x, y in coords]
        if len(points) < 3:
            continue

        polylines.append(
            LwPolyline(points=points, is_closed=True, layer=PARK_LAYER)
        )
        polyline_names.append(name)

names_found = sum(1 for n in polyline_names if n)
print(
    f"Prepared {len(polylines)} closed LwPolylines on layer '{PARK_LAYER}' "
    f"({names_found} with OSM names)"
)

# Refresh the snapshot before reading existing entities for removal.
time.sleep(0.2)
ocs.doc.refresh()

existing = [
    e for e in ocs.doc.entities()
    if getattr(e, "layer", None) == PARK_LAYER
    and type(e).__name__ == "LwPolyline"
]
if existing:
    print(f"Removing {len(existing)} existing polylines from layer '{PARK_LAYER}'")
    for e in existing:
        ocs.doc.remove(e.handle)
    time.sleep(0.2)

if polylines:
    handles = ocs.doc.add_many(polylines)
    print(f"Added {len(handles)} polylines")

    # Attach the OSM name as XDATA under the registered PYREPL app.
    named_count = 0
    failed_count = 0
    for handle, name in zip(handles, polyline_names):
        if name:
            ok = ocs.doc.write_record(handle, {
                "app_name": XDATA_APP,
                "values": [{"kind": "String", "value": name}],
            })
            if ok:
                named_count += 1
            else:
                failed_count += 1
                print(f"Warning: failed to attach XDATA to handle {handle}")
    print(f"Attached OSM names to {named_count} polylines via XDATA")
    if failed_count:
        print(
            f"Warning: {failed_count} XDATA writes failed "
            f"(layer locked or entity missing)"
        )
else:
    print("No park polygons to add.")

# Host drains plugin requests on a short timer; refresh the snapshot.
time.sleep(0.3)
ocs.doc.refresh()
print("Counts:", ocs.doc.counts())
