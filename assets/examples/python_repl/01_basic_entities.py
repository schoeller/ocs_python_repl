"""
Example: create basic entities through the Python REPL plugin.

Run inside an OpenCADStudio IPython session (after typing PYTHONSHELL):

    %pyimport assets/examples/python_repl/01_basic_entities.py

Supported entity kinds for creation/update: Point, Line, Circle, Arc,
Ellipse, Polyline, Polyline2D, Polyline3D, LwPolyline, Spline, MText.
"""
import time
from ocs.entities import (
    Point, Line, Circle, Arc, Ellipse,
    Polyline, Polyline2D, Polyline3D, LwPolyline, Spline, MText,
)

# Point ------------------------------------------------------------------------
p = Point(x=10.0, y=20.0, z=0.0)
point_handle = ocs.doc.add(p)
print(f"Added Point handle={point_handle}")

# Line -------------------------------------------------------------------------
l = Line(start=(0.0, 0.0, 0.0), end=(100.0, 50.0, 0.0))
line_handle = ocs.doc.add(l)
print(f"Added Line handle={line_handle}")

# Circle -----------------------------------------------------------------------
c = Circle(center=(50.0, 50.0, 0.0), radius=25.0)
circle_handle = ocs.doc.add(c)
print(f"Added Circle handle={circle_handle}")

# Arc --------------------------------------------------------------------------
a = Arc(center=(80.0, 80.0, 0.0), radius=20.0, start_angle=0.0, end_angle=90.0)
arc_handle = ocs.doc.add(a)
print(f"Added Arc handle={arc_handle}")

# Ellipse ----------------------------------------------------------------------
e = Ellipse(
    center=(120.0, 50.0, 0.0),
    major_axis=(40.0, 0.0, 0.0),
    minor_axis_ratio=0.5,
    start_parameter=0.0,
    end_parameter=6.283185307179586,
)
ellipse_handle = ocs.doc.add(e)
print(f"Added Ellipse handle={ellipse_handle}")

# Polyline ---------------------------------------------------------------------
pl = Polyline(points=[(0.0, 0.0, 0.0), (50.0, 0.0, 0.0), (50.0, 50.0, 0.0)], closed=True)
polyline_handle = ocs.doc.add(pl)
print(f"Added Polyline handle={polyline_handle}")

# LwPolyline -------------------------------------------------------------------
lw = LwPolyline(points=[(0.0, 100.0, 0.0), (100.0, 100.0, 0.0), (100.0, 150.0, 0.0)])
lw_handle = ocs.doc.add(lw)
print(f"Added LwPolyline handle={lw_handle}")

# Spline -----------------------------------------------------------------------
s = Spline(
    degree=3,
    control_points=[(0.0, 0.0, 0.0), (30.0, 50.0, 0.0), (60.0, 0.0, 0.0), (90.0, 50.0, 0.0)],
    knots=[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
)
spline_handle = ocs.doc.add(s)
print(f"Added Spline handle={spline_handle}")

# MText ------------------------------------------------------------------------
t = MText(text="Hello from Python", insertion=(10.0, 180.0, 0.0), height=10.0)
text_handle = ocs.doc.add(t)
print(f"Added MText handle={text_handle}")

# Wait for the host timer to drain the request, then refresh the snapshot.
time.sleep(0.2)
ocs.doc.refresh()
print("Counts:", ocs.doc.counts())
