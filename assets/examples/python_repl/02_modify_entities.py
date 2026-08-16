"""
Example: modify and remove entities through the Python REPL plugin.

Run inside an OpenCADStudio IPython session:

    %pyimport assets/examples/python_repl/02_modify_entities.py

ocs.doc.update(entity_dict) replaces the entity whose `handle` matches the
dictionary. ocs.doc.remove(handle) deletes it. All entity kinds supported for
creation are also supported for update.
"""
import time
from ocs.entities import Point, Line, Circle, Polyline, Spline, MText

# Add a few entities to work with.
point_handle = ocs.doc.add({"kind": "Point", **Point(x=10.0, y=10.0, z=0.0).__dict__})
line_handle = ocs.doc.add({"kind": "Line", **Line(start=(0.0, 0.0, 0.0), end=(10.0, 10.0, 0.0)).__dict__})
circle_handle = ocs.doc.add({"kind": "Circle", **Circle(center=(50.0, 50.0, 0.0), radius=10.0).__dict__})
polyline_handle = ocs.doc.add({
    "kind": "Polyline",
    **Polyline(points=[(0.0, 0.0, 0.0), (20.0, 0.0, 0.0), (20.0, 20.0, 0.0)]).__dict__
})
spline_handle = ocs.doc.add({
    "kind": "Spline",
    **Spline(
        degree=3,
        control_points=[(0.0, 0.0, 0.0), (10.0, 20.0, 0.0), (20.0, 0.0, 0.0)],
        knots=[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    ).__dict__
})
text_handle = ocs.doc.add({
    "kind": "MText",
    **MText(text="before", insertion=(5.0, 30.0, 0.0), height=2.0).__dict__
})

# Update: move the point (keep the same handle).
moved = Point(x=30.0, y=30.0, z=0.0)
moved.handle = point_handle
ok = ocs.doc.update({"kind": "Point", **moved.__dict__})
print(f"Updated Point handle={point_handle}: {ok}")

# Update: change the circle radius.
bigger = Circle(center=(50.0, 50.0, 0.0), radius=25.0)
bigger.handle = circle_handle
ok = ocs.doc.update({"kind": "Circle", **bigger.__dict__})
print(f"Updated Circle handle={circle_handle}: {ok}")

# Update: add a vertex to the polyline.
more_points = Polyline(
    points=[(0.0, 0.0, 0.0), (20.0, 0.0, 0.0), (20.0, 20.0, 0.0), (0.0, 20.0, 0.0)],
    closed=True,
)
more_points.handle = polyline_handle
ok = ocs.doc.update({"kind": "Polyline", **more_points.__dict__})
print(f"Updated Polyline handle={polyline_handle}: {ok}")

# Update: change spline control points.
bent = Spline(
    degree=3,
    control_points=[(0.0, 0.0, 0.0), (10.0, 30.0, 0.0), (20.0, 0.0, 0.0)],
    knots=[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
)
bent.handle = spline_handle
ok = ocs.doc.update({"kind": "Spline", **bent.__dict__})
print(f"Updated Spline handle={spline_handle}: {ok}")

# Update: change MText contents.
renamed = MText(text="after", insertion=(5.0, 30.0, 0.0), height=2.0)
renamed.handle = text_handle
ok = ocs.doc.update({"kind": "MText", **renamed.__dict__})
print(f"Updated MText handle={text_handle}: {ok}")

# Remove the line.
ok = ocs.doc.remove(line_handle)
print(f"Removed Line handle={line_handle}: {ok}")

# Wait for the host timer to drain the requests, then refresh.
time.sleep(0.2)
ocs.doc.refresh()
print("Counts:", ocs.doc.counts())

# Show the updated point.
entity = ocs.doc.entity(point_handle)
print("Updated entity:", entity)
