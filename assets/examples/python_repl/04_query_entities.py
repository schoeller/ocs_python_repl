"""
Example: read entities back from the shared snapshot.

Run inside an OpenCADStudio IPython session:

    %pyimport assets/examples/python_repl/04_query_entities.py

Refresh the snapshot after the host has republished it, then use:

    ocs.doc.counts()        -> dict of entity kind -> count
    ocs.doc.entity(handle)  -> entity dataclass or None
    ocs.doc.entities()      -> list of all entity dataclasses
    ocs.doc.layers()        -> dict of handle -> layer name
"""
import time

# Make sure the snapshot is up to date.
time.sleep(0.2)
ocs.doc.refresh()

print("Entity counts:", ocs.doc.counts())
print("Layers:", ocs.doc.layers())

# List every entity.
entities = ocs.doc.entities()
print(f"Total entities: {len(entities)}")
for e in entities[:5]:
    print(f"  handle={e.handle} kind={type(e).__name__} layer={e.layer}")

# Look up a specific entity by handle if one exists.
if entities:
    first = entities[0]
    found = ocs.doc.entity(first.handle)
    print(f"Lookup handle {first.handle}: {found}")
