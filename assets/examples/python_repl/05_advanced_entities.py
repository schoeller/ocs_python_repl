"""
Example: advanced entity kinds and support types.

Run inside an OpenCADStudio IPython session:

    %pyimport assets/examples/python_repl/05_advanced_entities.py

Shows creation of Ellipse, Polyline2D, Polyline3D, LwPolyline, Spline, and
MText, plus the support classes Vector2, Vector3, Color, Layer, and XDataValue.
"""
import time
from ocs.entities import (
    Ellipse, Polyline2D, Polyline3D, LwPolyline, Spline, MText,
    Vector3, Color, Layer, XDataValue,
)

# Ellipse ----------------------------------------------------------------------
e = Ellipse(
    center=(100.0, 100.0, 0.0),
    major_axis=(50.0, 0.0, 0.0),
    minor_axis_ratio=0.4,
    start_parameter=0.0,
    end_parameter=6.283185307179586,
)
ellipse_handle = ocs.doc.add(e)
print(f"Added Ellipse handle={ellipse_handle}")

# Polyline2D -------------------------------------------------------------------
p2d = Polyline2D(
    points=[(0.0, 0.0, 0.0), (30.0, 0.0, 0.0), (30.0, 20.0, 0.0)],
    closed=True,
    elevation=5.0,
)
p2d_handle = ocs.doc.add(p2d)
print(f"Added Polyline2D handle={p2d_handle}")

# Polyline3D --------------------------------------------------------------------
p3d = Polyline3D(
    points=[(0.0, 0.0, 0.0), (0.0, 0.0, 10.0), (10.0, 0.0, 10.0)],
    closed=False,
)
p3d_handle = ocs.doc.add(p3d)
print(f"Added Polyline3D handle={p3d_handle}")

# LwPolyline --------------------------------------------------------------------
lw = LwPolyline(
    points=[(200.0, 0.0, 0.0), (250.0, 0.0, 0.0), (250.0, 50.0, 0.0)],
    closed=True,
    constant_width=0.5,
)
lw_handle = ocs.doc.add(lw)
print(f"Added LwPolyline handle={lw_handle}")

# Spline -----------------------------------------------------------------------
s = Spline(
    degree=3,
    control_points=[(0.0, 50.0, 0.0), (30.0, 100.0, 0.0), (60.0, 50.0, 0.0), (90.0, 100.0, 0.0)],
    knots=[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    weights=[1.0, 1.0, 1.0, 1.0],
)
spline_handle = ocs.doc.add(s)
print(f"Added Spline handle={spline_handle}")

# MText ------------------------------------------------------------------------
t = MText(text="Advanced entities", insertion=(10.0, 150.0, 0.0), height=5.0)
text_handle = ocs.doc.add(t)
print(f"Added MText handle={text_handle}")

# Support types are available for building dictionaries manually.
v = Vector3(x=1.0, y=2.0, z=3.0)
c = Color(kind="Rgb", r=255, g=0, b=0)
layer = Layer(name="MyLayer", color=c)
xv = XDataValue(kind="String", value="app data")
print(f"Support types: Vector3={v}, Color={c}, Layer={layer.name}, XDataValue={xv.value}")

# Wait for the host timer to drain the requests, then refresh.
time.sleep(0.2)
ocs.doc.refresh()
print("Counts:", ocs.doc.counts())
