# modul_render

Core rendering infrastructure: render targets, pipelines, bind groups, and sequences.

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
└── Present
```

## Render Targets

### RenderTarget Trait

Common interface for renderable surfaces. Provides methods for:
- Accessing texture/view
- Configuring color/depth/stencil attachments
- Beginning render passes

### SurfaceRenderTarget (Component)

Window surface rendering:
- Color always present
- Optional depth/stencil
- Present mode (VSync, NoVsync, Auto)
- Auto-reconfigures on resize

### OffscreenRenderTarget (Component)

Texture-based rendering:
- Optional color, depth/stencil, multisampling
- Configurable formats and usages
- Deferred changes applied in `ApplyOffscreenTargetsSystemSet`

### Configuration

- `RenderTargetColorConfig` - clear color, multisample, format
- `RenderTargetDepthStencilConfig` - format, clear values
- `RenderTargetMultisampleConfig` - MSAA samples

## Pipeline Management

### RenderPipelineManager

Caches pipelines for different render target configurations:
```rust
let pipeline = manager.get(device, color_fmt, depth_fmt, samples);
let pipeline = manager.get_compatible(device, &render_target);
```

### GenericRenderPipelineDescriptor

Pipeline descriptor without format info. Create pipelines for any target at runtime:
```rust
let desc = GenericRenderPipelineDescriptor { ... };
let manager = RenderPipelineManager::new(desc, provider);
```

### RenderPipelineResourceProvider

Trait for providing shader modules and layouts.

## Bind Group Composition

The composition system lets you define self-contained bind groups — each owning its GPU layout and WGSL shader declarations — then compose them into a final pipeline. Each bind group is an independent module: swap or reuse camera, material, voxel data, etc. without touching the rest of the renderer.

### Traits

**BindGroupLayoutProvider** — provides a bind group's layout descriptor and WGSL library. Use `#BIND_GROUP` as a placeholder for the group index.

```rust
impl BindGroupLayoutProvider for MyBindGroup {
    fn layout(&self) -> BindGroupLayoutDescriptor { ... }
    fn library(&self) -> &str {
        "@group(#BIND_GROUP) @binding(0)\nvar<uniform> view: mat4x4<f32>;"
    }
}
```

**ConstBindGroupLayoutProvider** — for compile-time-known layouts. Auto-implements `BindGroupLayoutProvider`.

**BindGroupProvider** — runtime trait providing `&BindGroup` for render passes. Not all providers have uniforms; this trait is the minimal render-time interface.

### SimpleBindGroupLayoutBuilder

Declaratively define bind groups with arbitrary bindings and typed uniforms:

```rust
let mut builder = SimpleBindGroupLayoutBuilder::new();

// Manual binding (textures, samplers, storage buffers, etc.)
let tex = builder.add_entry(
    "my_texture".into(), "texture_2d<f32>".into(),
    ShaderStages::FRAGMENT, binding_type, None,
);

// Typed uniform — auto-generates layout entry + WGSL var<uniform> declaration
let color = builder.add_uniform::<Vec4Uniform>("tint_color".into());

// build() takes &Device — creates BindGroupLayout, result is immutable
let layout_provider = builder.build(&device);
```

### SimpleBindGroupProvider

Created from a layout provider. Holds the `BindGroup` and uniform `Buffer`s:

```rust
// Allocates uniform buffers, binds everything into a BindGroup
let provider = layout_provider.build_bind_group(&device, &[(tex, texture_resource)]);

// Type-safe uniform updates at runtime
provider.set_uniform(&queue, &color, [1.0, 0.0, 0.0, 1.0]);
```

`UniformEntry<Ty>` carries the type — you can only pass `Ty::Resource`, preventing mismatched writes.

### WgslUniformType & UniformType

`WgslUniformType` enum — known WGSL types: `Float`, `Vec2`, `Vec3`, `Vec4`, `Mat3x3`, `Mat4x4`. Provides type name and byte size.

`UniformType` trait — maps a Rust type to a WGSL uniform:

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

The enum avoids `dyn` trait object issues — the builder stores `WgslUniformType` values, while type safety comes from the generic `UniformEntry<Ty>` handle.

### PipelineLayoutComposer

Combines multiple `BindGroupLayoutProvider`s into a single pipeline layout and composed shader:

```rust
let mut composer = PipelineLayoutComposer::new();
composer
    .add_bind_group_layout(camera_bg)   // group 0
    .add_bind_group_layout(material_bg); // group 1

let layout = composer.compose_pipeline_layout(&device);
let shader = composer.compose_shader(&device, &mut naga_composer, desc)?;
```

Each provider's WGSL library is added as a `naga_oil` composable module with `#BIND_GROUP` replaced by the actual index. Results are cached and only recomposed when providers change.

### ComposedPipelineResourceProvider

Implements `RenderPipelineResourceProvider` using a `PipelineLayoutComposer`, bridging the composition system into pipeline management. Stores asset IDs for the composer, shader descriptor, and naga_oil `Composer`.

## Sequences

### Sequence (Asset)

Ordered render operations with automatic resolve handling:
```rust
let sequence = SequenceBuilder::new()
    .add(ClearNext::color([0.0, 0.0, 0.0, 1.0]))
    .add(DrawTriangle { ... })
    .build();
```

Automatically inserts resolve operations when:
- Reading a target that was previously written
- After all writes to ensure final state

### OperationBuilder Trait

Define render operations:
```rust
impl OperationBuilder for MyOp {
    fn reads(&self) -> Vec<RenderTargetSource>;
    fn writes(&self) -> Vec<RenderTargetSource>;
    fn build(self, world: &mut World) -> Box<dyn Operation>;
}
```

### Operation Trait

Execute render commands:
```rust
impl Operation for MyOperation {
    fn run(&mut self, world: &mut World);
}
```

### RunningSequenceQueue

Queue sequences for execution:
```rust
queue.push(sequence_id);  // Runs during Draw schedule
```

### Built-in Operations

- `ClearNext` - Schedule color clear
- `EmptyPass` - Empty pass (for resolve)

## Files

- `lib.rs` - Schedule setup, re-exports
- `render_target.rs` - RenderTarget trait and implementations
- `pipeline_manager.rs` - Pipeline caching and generic descriptors
- `bind_group_composition.rs` - Shader/layout composition
- `sequence.rs` - Render operation scheduling
- `sequence/basic.rs` - Built-in operations
