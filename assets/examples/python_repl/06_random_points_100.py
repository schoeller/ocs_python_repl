# Create 100 random points in a 100 x 100 area.
# Run from an IPython REPL started with PYTHONSHELL:
#   %pyimport path/to/06_random_points_100.py
import random
import time

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
print(f"Added {len(handles)} random points")

# The host drains plugin requests on a 5 ms timer; wait briefly, then refresh.
time.sleep(0.2)
ocs.doc.refresh()
print(ocs.doc.counts())
