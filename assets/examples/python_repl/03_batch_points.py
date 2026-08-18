"""
Example: add many points in one batch request.

Run inside an OpenCADStudio IPython session:

    %pyimport assets/examples/python_repl/03_batch_points.py

ocs.doc.add_many(list) is much faster than calling ocs.doc.add(dict) in a loop
because the whole batch travels in one plugin request and the host publishes
only one snapshot.
"""
import random
import time
from ocs.entities import Point

N = 5000
random.seed(42)
points = [
    Point(
        x=random.uniform(-500.0, 500.0),
        y=random.uniform(-500.0, 500.0),
        z=0.0,
    )
    for _ in range(N)
]

t0 = time.perf_counter()
handles = ocs.doc.add_many(points)
t1 = time.perf_counter()

print(f"Submitted {N} points in {t1 - t0:.3f}s ({N / (t1 - t0):,.0f} points/s)")
print(f"Returned {len(handles)} handles, first = {handles[0]}")

# Wait for the host timer to drain the request, then refresh.
time.sleep(0.2)
ocs.doc.refresh()
counts = ocs.doc.counts()
print(counts)
print(f"Total entities in document: {sum(counts.values())}")
