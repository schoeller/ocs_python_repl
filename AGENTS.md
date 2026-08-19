# Agentic coding rules: `ocs_python_repl` entity bindings

This file is for autonomous agents (and future maintainers) who add or modify
Python entity bindings in the `crates/ocs_python_repl` crate.

## Golden rule

The `ocs_plugin_api` embedded type registry is the single source of truth for
`acadrust` field types. `crud_manifest.json` is only a public-API projection.
Never hand-write a full entity binding when the registry plus a small manifest
override can generate it.

## Before touching entity code

1. Read the current registry for the entity and its field types.
   The registry is written to `crates/ocs_plugin_api/target/<profile>/build/ocs_plugin_api-<hash>/out/type_registry.json`
   after a successful build. You can also search the most recent build output:

   ```powershell
   Get-ChildItem crates/ocs_plugin_api/target/debug/build/ocs_plugin_api-*/out/type_registry.json
   ```

2. Read the current manifest:
   `crates/ocs_python_repl/crud_manifest.json`.

3. Read the generator entry points:
   `crates/ocs_python_repl/build/generate.rs` and
   `crates/ocs_python_repl/build.rs`.

4. Read the conversion helpers:
   `crates/ocs_python_repl/src/python_ext.rs`.

## How to add a new entity kind

1. **Ensure the type is in the registry.** Add it to the allow-list in
   `crates/ocs_plugin_api/build.rs` if it is missing:

   ```rust
   ("MyEntity", trace::<acadrust::entities::MyEntity>),
   ```

   If the entity contains enums that `serde-reflection` has not seen, add sample
   values in `add_enum_samples`.

2. **Add the entity to the central filter.** Open
   `crates/ocs_python_repl/crud_manifest.json` and append the entity to
   `type_filter`.

3. **Run a build.** This regenerates `OUT_DIR/python/ocs/entities.py`,
   `entities.pyi`, and `OUT_DIR/entity_crud.rs`.

4. **Inspect the generated code.** If the registry produces the right public
   shape, no override is needed. If not, add a minimal override under
   `overrides`.

5. **Add a round-trip test.** In `crates/ocs_python_repl/src/python_ext.rs`,
   copy an existing test (e.g. `point_roundtrips`) and adapt it to the new
   entity. The test must construct the Rust entity, call `roundtrip`, and assert
   the fields that matter.

6. **Run the test suite:**

   ```powershell
   cargo test -p ocs_python_repl --manifest-path crates/ocs_python_repl/Cargo.toml
   ```

   Do not consider the task done until this command passes.

7. **Run clippy:**

   ```powershell
   cargo clippy -p ocs_python_repl --manifest-path crates/ocs_python_repl/Cargo.toml -- -D warnings
   ```

   The generator is lint-clean; any new warnings must be fixed in the generator,
   not by silencing them in generated code.

## When to use an override

Only add entries under `overrides` when the registry cannot express the desired
public API. Common reasons:

- **Constructor choice:** the Rust type needs `T::new()` or `T::from_coords(...)`
  instead of `T::default()`.
- **Flattening:** a struct field (e.g. `SplineFlags`) should appear as multiple
  Python fields.
- **Renaming:** a registry field name is awkward for Python users
  (e.g. `value` -> `text` for MText).
- **Custom conversion:** a field is a bitflags struct, a vertex list exposed as
  plain tuples, or otherwise not a 1:1 mapping.
- **Default value:** the registry default is wrong for the public API
  (e.g. `normal` should default to `(0.0, 0.0, 1.0)`).

Do **not** add overrides for:

- Plain numeric/string/bool fields.
- `Vector3` / `Vector2` fields (handled automatically as tuples).
- Unit enum fields (handled automatically as strings).
- Nested struct or data-enum fields (handled automatically via generated helper
  classes).

## Override examples

### Constructor override

```json
"MyEntity": {
  "constructor": { "kind": "new" }
}
```

Kinds: `default` (uses `T::default()`), `new` (uses `T::new()`), `from_coords`
(uses `T::from_coords(args...)`).

### Field rename and default

```json
"MText": {
  "constructor": { "kind": "new" },
  "fields": {
    "value": { "python_name": "text" },
    "insertion_point": { "python_name": "insertion" },
    "normal": { "default": "(0.0, 0.0, 1.0)" }
  }
}
```

### Flatten a struct into booleans

```json
"Spline": {
  "constructor": { "kind": "default" },
  "fields": {
    "flags": { "flatten": ["closed", "periodic", "rational", "planar", "linear"] }
  }
}
```

### Custom Rust getter/setter for a vertex list

```json
"Polyline": {
  "constructor": { "kind": "default" },
  "fields": {
    "vertices": {
      "python_name": "points",
      "python_type": "List[Tuple[float, float, float]]",
      "default": "[]",
      "rust_getter": "p.vertices.iter().map(|v| v3_tuple(&v.location)).collect::<Vec<_>>()",
      "rust_setter": "if let Some(pts) = entity_attr(entity, \"points\") { p.vertices = point_list(&pts)?.into_iter().map(acadrust::entities::Vertex3D::new).collect(); }"
    }
  }
}
```

Custom `rust_getter` and `rust_setter` are the escape hatch. When you use them,
add or reuse a helper in `src/python_ext.rs` if the conversion is non-trivial,
and document why the override exists in `manual_overrides`.

## How to update an existing entity

1. If you only need to change the public API shape, edit
   `crud_manifest.json`, not the generated files.

2. If the Rust type changed in `acadrust`, ensure the registry is updated first
   (step 1 of "How to add a new entity kind").

3. If a new field is not handled automatically, add the smallest possible
   override.

4. Update or add a round-trip test if behavior changed.

## Code style for the generator

- Keep generator functions clippy-clean for `-D warnings`.
- Avoid generating `format!("...")` for static strings; use `.to_string()`.
- Avoid identity closures (`|v| v`) and needless borrows (`&v` where `v` is
  already a reference).
- Prefer `to_vec()` over `iter().copied().collect::<Vec<_>>()` for numeric
  slices.
- Keep custom Rust expressions in `crud_manifest.json` as short as possible;
  move reusable logic into `src/python_ext.rs` helpers.

## Common pitfalls

- **Build order:** `ocs_python_repl` reads the registry produced by
  `ocs_plugin_api`. `build.rs` now validates that the selected registry
  contains every entity in `crud_manifest.json.type_filter` plus
  `EntityCommon`; if the build picks a stale artifact, delete
  `crates/ocs_python_repl/target` and rebuild both crates.

- **Type registry selection:** `ocs_python_repl/build.rs` searches
  `crates/ocs_python_repl/target/<profile>/build/ocs_plugin_api-*/out/type_registry.json`
  and prefers the newest registry that contains all required types. If you
  build from the workspace root, the paths differ; build with the manifest
  path:

  ```powershell
  cargo build --manifest-path crates/ocs_python_repl/Cargo.toml
  ```

- **Unit enum variants:** enums with only unit variants are exposed as Python
  strings. Data enums / structs are exposed as generated dataclasses.

- **Option<T> fields:** the generator emits `Option<T>` directly for scalars and
  uses `.as_ref().map(...)` for non-Copy types. Custom overrides must respect
  this or provide their own correct conversion.

- **Dataclass instances vs dicts:** `py_to_entity` accepts both. Dicts need a
  `kind` key; dataclass instances use their class name automatically. Examples
   should prefer passing dataclass instances directly.

## Verification checklist

- [ ] `cargo test -p ocs_python_repl --manifest-path crates/ocs_python_repl/Cargo.toml` passes.
- [ ] `cargo clippy -p ocs_python_repl --manifest-path crates/ocs_python_repl/Cargo.toml -- -D warnings` passes.
- [ ] `crud_manifest.json` is valid JSON and the central `type_filter` is updated.
- [ ] New or changed entities have a round-trip test in `src/python_ext.rs`.
- [ ] Example scripts under `assets/examples/python_repl/` are updated if the
      public API changed.
- [ ] README.md is updated if the architecture, override mechanism, or examples
      changed.
- [ ] `manual_overrides` documents any binding that still requires hand-written
      code and explains why.
