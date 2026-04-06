# modul_render

Core rendering infrastructure: render targets, pipelines, bind groups, and sequences.

> **Best practice:** import wgpu types via `modul_core::wgpu` rather than
> a direct dependency, so your code automatically inherits modul's pinned
> major version.

## Schedule Structure

Extends the `Redraw` schedule with nested schedules:

```
Redraw
├── RenderSystemSet
│   ├── create_surface_targets
│   ├── handle_events
│   └── draw →
│       ├── PreSynchronize
│       ├── Synchronize
│       ├── PostSynchronize
│       ├── PreDraw
│       ├── Draw
│       │   ├── ApplyOffscreenTargetsSystemSet
│       │   └── SequenceRunnerSet
│       └── PostDraw
└── present_surface_targets, request_redraws
```

## Render Targets

### `RenderTarget` trait

Common interface for renderable surfaces. Provides methods for:
- Querying size, sample count, clear values
- Accessing the primary texture / view, depth-stencil texture / view
- Reading and scheduling color / depth-stencil configs
- Beginning render passes (resolving, non-resolving, ending)

### `SurfaceRenderTarget` (Component)

Window surface rendering:
- Color attachment always present
- Optional depth/stencil
- Present mode configuration (`PresentMode::AutoVsync`, `AutoNoVsync`, etc.)
- Auto-reconfigures on resize
- `update(device, surface)` returns a `SurfaceUpdateStatus`:
  - `Ready` — texture acquired, render normally
  - `ReadySuboptimal` — texture acquired but surface is suboptimal
    (mid-resize); render this frame and reconfigure on the next call
  - `Skipped` — `Outdated` / `Lost` / `Timeout` / `Occluded` — caller
    should request another redraw
  - `Failed` — fatal validation error; caller should exit

### `OffscreenRenderTarget` (Component)

Texture-based rendering:
- Optional color, depth/stencil, multisampling
- Configurable formats and usages
- Deferred changes applied in `ApplyOffscreenTargetsSystemSet`

### Configuration types

- `RenderTargetColorConfig` — clear color, multisample config, usages, format override
- `RenderTargetDepthStencilConfig` — clear depth/stencil, usages, format
- `RenderTargetMultisampleConfig` — MSAA sample count
- `SurfaceRenderTargetConfig` — surface-specific (present mode, frame latency, ...)
- `OffscreenRenderTargetConfig` — offscreen-specific (size, ...)

## Pipeline Management

### `RenderPipelineManager`

Caches one `RenderPipeline` per `(color_format, depth_stencil_format,
sample_count)` tuple — the same generic pipeline definition produces
different concrete instances for different render targets.

```rust
let pipeline = manager.get(&mut world, &PipelineParameters {
    color_format: Some(TextureFormat::Bgra8UnormSrgb),
    depth_stencil_format: None,
    sample_count: 4,
});

// Convenience: pick parameters from a target automatically.
let pipeline = manager.get_compatible(render_target_source, &mut world);
```

The cache lives inside the manager; `get` is the canonical entry point.
This is *not* a binary shader cache — for cross-process pipeline cache
warming, use wgpu's `PipelineCache` directly (not yet wired up).

### `GenericRenderPipelineDescriptor`

Pipeline descriptor without format / sample count info, allowing the
same definition to be instantiated for any render target at runtime.
Construct it once and pass it to `RenderPipelineManager::new`.

### `RenderPipelineResourceProvider` trait

Abstracts the source of a pipeline's `PipelineLayout` and shader modules
(vertex / fragment). The simplest implementation,
`DirectRenderPipelineResourceProvider`, takes asset IDs.

## Bind Group Composition

A system for defining self-contained bind groups — each owning its GPU
layout, WGSL shader declarations, and (for runtime layouts) buffer
management — then composing them into a final pipeline. Each bind group
is an independent module: swap or reuse camera, material, voxel data,
etc. without touching the rest of the renderer.

### Traits

**`BindGroupLayoutProvider`** — provides a bind group's layout and a WGSL
library snippet declaring its bindings. Use `#BIND_GROUP` as a placeholder
for the group index — the composer substitutes the actual index when
composing.

```rust
impl BindGroupLayoutProvider for MyBindGroup {
    fn layout(&self) -> &wgpu::BindGroupLayout { &self.layout }
    fn library(&self) -> &str {
        "@group(#BIND_GROUP) @binding(0)\nvar<uniform> view: mat4x4<f32>;"
    }
}
```

**`BindGroupLayoutDef`** — for compile-time-known layouts. Provides a
`const LAYOUT: &'static BindGroupLayoutDescriptor` and a `const LIBRARY: &str`.
Use with `CachedBindGroupLayout<P>` and `BindGroupLayoutInitPlugin<P>`
for zero-boilerplate static layouts.

**`BindGroupProvider`** — runtime trait providing `&BindGroup` for render
passes. The minimal render-time interface; not all providers also have
mutable uniforms.

### `CachedBindGroupLayout<P>`

Resource that caches a `BindGroupLayout` created from a static
`BindGroupLayoutDef`. Insert via `BindGroupLayoutInitPlugin<P>`, which
runs the layout creation system once during `Init`.

```rust
app.add_plugins(BindGroupLayoutInitPlugin::<MyLayoutDef>::new());
```

### `SimpleBindGroupLayoutBuilder`

Runtime builder for bind groups with arbitrary bindings and typed uniforms:

```rust
let mut builder = SimpleBindGroupLayoutBuilder::new();

// Manual binding (textures, samplers, storage buffers, etc.)
let tex = builder.add_entry(
    "my_texture".into(),
    "texture_2d<f32>".into(),
    ShaderStages::FRAGMENT,
    binding_type,
    None,
);

// Typed uniform — auto-generates layout entry + WGSL var<uniform> declaration
let color = builder.add_uniform::<Vec4Uniform>("tint_color".into());

// build() takes &Device — creates the BindGroupLayout
let layout_provider = builder.build(&device);
```

### `SimpleBindGroupProvider`

Created by binding resources to a `SimpleBindGroupLayoutProvider`. Holds
the `BindGroup` and uniform `Buffer`s.

```rust
let provider = layout_provider.build_bind_group(
    &device,
    &[(tex, BindingResource::TextureView(&texture_view))],
);

// Type-safe uniform updates at runtime
provider.set_uniform(&queue, &color, [1.0, 0.0, 0.0, 1.0]);
```

`UniformEntry<Ty>` carries the type so `set_uniform` will only accept
`Ty::Resource` — preventing mismatched writes at compile time.

### `WgslUniformType` and `UniformType`

`WgslUniformType` enum lists known WGSL uniform types: `Float`, `Vec2`,
`Vec3`, `Vec4`, `Mat3x3`, `Mat4x4`. Provides `wgsl_type_name()` and
`byte_size()`.

`UniformType` trait maps a Rust marker type to a WGSL uniform:

```rust
struct Vec4Uniform;
impl UniformType for Vec4Uniform {
    type Resource = [f32; 4];
    fn set_bytes(res: Self::Resource, bytes: &mut [u8]) {
        bytes.copy_from_slice(bytemuck::cast_slice(&res));
    }
    fn wgsl_uniform_type() -> WgslUniformType { WgslUniformType::Vec4 }
}
```

The enum sidesteps `dyn` issues — the builder stores `WgslUniformType`
values, while type safety comes from the generic `UniformEntry<Ty>` handle.

### `PipelineLayoutComposer`

Combines multiple `BindGroupLayoutProvider`s into a single
`PipelineLayout` and a composed `ShaderModule`. WGSL libraries from each
provider are concatenated, with `#BIND_GROUP` replaced by the actual
group index. Additional WGSL snippets can be prepended before the main
shader source.

```rust
let mut composer = PipelineLayoutComposer::new();
composer
    .add_bind_group(camera_provider)   // group 0
    .add_bind_group(material_provider) // group 1
    .add_snippet(LIGHTING_UTILS);

let layout = composer.compose_pipeline_layout(&device);
let shader = composer.compose_shader(&device, "my_shader", MAIN_WGSL);
```

Results are cached internally; mutating the composer (e.g.
`add_bind_group`) invalidates the cache.

> **Note:** the composer used to depend on `naga_oil` for module merging.
> Since the bind group rewrite (commit `fd34a17`) it does plain string
> concatenation with placeholder substitution — simpler and removes a
> heavy dependency.

## Sequences

### `Sequence` (Asset)

Ordered render operations with automatic resolve handling:

```rust
let sequence = SequenceBuilder::new()
    .add(ClearNext { render_target })
    .add(MyDrawOp { ... })
    .finish(&mut sequence_assets);
```

Automatically inserts resolve operations when reading a target that was
previously written, and after all writes to ensure final state.

### `OperationBuilder` trait

Define render operations:

```rust
impl OperationBuilder for MyOp {
    fn reading(&self) -> Vec<RenderTargetSource> { ... }
    fn writing(&self) -> Vec<RenderTargetSource> { ... }
    fn finish(self, world: &World, device: &Device) -> impl Operation + 'static { ... }
}
```

### `Operation` trait

Execute render commands. Receives `&mut CommandEncoder` and `&mut World`.

### `RunningSequenceQueue`

Resource holding the sequence asset IDs that should run each frame:

```rust
commands.insert_resource(RunningSequenceQueue(SequenceQueue(vec![sequence_id])));
```

Sequences are executed during the `Draw` schedule in `SequenceRunnerSet`.

### Built-in operations

- `ClearNext` — schedule a color clear for the next pass on a target
- `EmptyPass` — empty render pass (useful for forcing a resolve)

## Files

- `lib.rs` — schedule setup, plugin, system definitions
- `render_target.rs` — `RenderTarget` trait, `SurfaceRenderTarget`, `OffscreenRenderTarget`
- `pipeline_manager.rs` — `RenderPipelineManager`, `GenericRenderPipelineDescriptor`
- `bind_group_composition.rs` — bind group layout / provider system, `PipelineLayoutComposer`
- `sequence.rs` — render operation scheduling, `Sequence`, `OperationBuilder`
- `sequence/basic.rs` — built-in operations (`ClearNext`, `EmptyPass`)
