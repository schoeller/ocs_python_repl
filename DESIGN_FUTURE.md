# Python REPL binding generation — status and future work

## Status: implemented

The design described in the original version of this document is now
implemented. `crates/ocs_python_repl/build/generate.rs` reads the
`ocs_plugin_api` embedded type registry and `crud_manifest.json` at build time
and generates:

- `OUT_DIR/python/ocs/entities.py`
- `OUT_DIR/python/ocs/entities.pyi`
- `OUT_DIR/entity_crud.rs` (included by `src/python_ext.rs`)

The registry is the single source of truth. `crud_manifest.json` only supplies
constructors, renames, flattens, defaults, and custom Rust getter/setter
expressions where the public API must differ from the raw registry.

See `crates/ocs_python_repl/README.md` for the architecture diagram, override
examples, and usage instructions. See `AGENTS.md` for rules on adding or
modifying entity bindings.

## Remaining future work

1. **Runtime registry reloading.** Today the registry is baked in at build time.
   In the future the host could publish a small registry blob alongside the
   snapshot so bindings stay in sync with a newer `acadrust` without a plugin
   rebuild.

2. **Cover more entity kinds.** The current `type_filter` includes the most
   common entity types. Expand it as `acadrust` adds support for dimensions,
   hatches, blocks, attributes, etc.

3. **Typed helper accessors.** Generated helper classes (`Color`, `Layer`,
   `XDataValue`) are already emitted. Consider richer Python ergonomics, such as
   `Color.rgb()` or `Layer.by_name(...)` helpers, when the registry can express
   them.

4. **Enum-set and bitflags support.** A few fields still need manual overrides
   because the registry exposes bitflags as opaque integers. When the registry
   grows a `Bitflags` kind, remove those overrides.

5. **Generated `counts()` and `entity_to_py` fallback.** `Document::counts()`
   still hardcodes entity kind names. Generate this mapping from `type_filter`
   so new entity kinds are counted automatically.

6. **Python-side validation.** Generate `__post_init__` validation or type hints
   that catch common mistakes (e.g. passing a 2-tuple where a 3-tuple is
   expected) before the request reaches Rust.
