# Modul Engine Documentation

A modular game/rendering engine built on Bevy ECS with wgpu. The goal is to provide a renderer that is simple and easy to modify at every level of abstraction.

> **Best practice for downstream consumers:** import wgpu and winit types
> via `modul_core::wgpu` and `modul_core::winit` rather than adding them
> to your own `Cargo.toml`. modul re-exports them so your code automatically
> inherits the same pinned major version. Mixing modul's wgpu with your
> own causes confusing "expected `wgpu::Device`, found `wgpu::Device`"
> errors at compile time.

## Architecture Overview

The engine is organized as a Cargo workspace with six interconnected crates:

```
modul_core       (foundation: event loop, windows, GPU initialization)
     ↓
modul_util       (utilities, exit handling, re-exports)
     ↓
modul_asset      (generic asset storage system)
     ↓
modul_render     (render targets, pipelines, sequences, bind groups)
     ↓
modul_texture    (image loading, GPU texture management)

modul_sprite     (reserved for future sprite system)
```

---

## modul_core

**Purpose:** Foundation crate providing the event loop, window management, and GPU initialization.

### Entry Point

```rust
run_app(graphics_initializer, setup_fn)
```

The main entry point. Creates the winit event loop, initializes graphics via the provided initializer, then runs the setup function followed by the main loop.

### Schedules

| Schedule | When it runs |
|----------|--------------|
| `PreInit` | Before graphics/window setup |
| `Init` | After graphics setup, used for app initialization |
| `Redraw` | Main update loop, runs when window requests redraw |

### Key Resources

| Resource | Description |
|----------|-------------|
| `RenderContext` | Bundled wgpu `instance` / `adapter` / `device` / `queue`. Take `Res<RenderContext>` in any system that touches wgpu. |
| `SurfaceFormat` | Default surface texture format chosen by the active `GraphicsInitializer` |
| `WindowMap` | Maps winit `WindowId` to ECS `Entity` |
| `EventBuffer` | winit events accumulated since the last `Redraw` |
| `ShouldExit` | Insert this resource to exit at the end of the current `Redraw` |

### Window Components

| Component | Description |
|-----------|-------------|
| `WindowComponent` | Wraps winit Window + wgpu Surface |
| `MainWindow` | Marker for the primary window |
| `ImportantWindow` | App exits when all important windows close |
| `UpdatingWindow` | Window that triggers redraw requests |
| `InitialWindowConfig` | Configuration for spawning new windows |

### Graphics Initialization

The `GraphicsInitializer` trait allows customizing GPU setup. `DefaultGraphicsInitializer` provides sensible defaults for device, adapter, and surface creation, and exposes:

- `power_preference` — `wgpu::PowerPreference`
- `window_attribs` — `winit::window::WindowAttributes`
- `required_features` — `wgpu::Features` to opt into beyond the spec minimum
- `required_limits` — `wgpu::Limits` to opt into higher resource limits

It implements `Default` so use struct-update syntax to set only the fields you care about:

```rust
DefaultGraphicsInitializer {
    window_attribs: WindowAttributes::default().with_title("hello"),
    required_features: wgpu::Features::PUSH_CONSTANTS,
    ..Default::default()
}
```

The trait also has a `pick_surface_format(&self, caps)` method with a default implementation that picks the first sRGB format. Override it to support HDR or linear pipelines.

---

## modul_util

**Purpose:** Small utilities and convenience plugins.

### ExitPlugin

Automatically exits the application when the main window receives a close request.

### Utilities

- `binsearch(f, range)` - Binary search for finding valid parameter values
- Re-exports `HashMap` and `HashSet` from `hashbrown`

---

## modul_asset

**Purpose:** Generic asset storage and management system.

### Core Types

**`AssetId<T>`** - A copyable, type-safe handle to an asset. Uses a simple usize internally.

**`Assets<T>`** - Resource holding all assets of a given type:
```rust
assets.add(value)       // Returns AssetId<T>
assets.get(id)          // Option<&T>
assets.get_mut(id)      // Option<&mut T>
assets.replace(id, val) // Replace existing
assets.remove(id)       // Remove and return
```

**`AssetMap<K, V>`** - HashMap keyed by `AssetId<K>` for associating metadata with assets.

### World Extensions

`AssetWorldExt` trait adds convenience methods to `World`:
- `world.add_asset::<T>(value)` - Add asset
- `world.get_asset::<T>(id)` - Get reference
- `world.asset_mut::<T, R>(id, |asset| ...)` - Mutable access with closure
- `world.with_asset::<T, R>(id, |asset| ...)` - Immutable access with closure
- `world.asset_scope::<T, R>(|assets| ...)` - Access entire Assets<T> resource

### App Extensions

`AssetAppExt` trait adds:
- `app.init_assets::<T>()` - Initialize asset storage during setup

---

## modul_render

**Purpose:** The core rendering infrastructure including render targets, pipeline management, bind group composition, and render sequences.

### Schedules & Ordering

The render module extends the `Redraw` schedule with a structured pipeline:

```
Redraw
├── RenderSystemSet
│   ├── create_surface_targets
│   ├── handle_events
│   └── draw → runs nested schedules:
│       ├── PreSynchronize
│       ├── Synchronize
│       ├── PostSynchronize
│       ├── PreDraw
│       ├── Draw
│       │   ├── ApplyOffscreenTargetsSystemSet
│       │   └── SequenceRunnerSet
│       └── PostDraw
└── Present & request redraws
```

### Render Targets

Two types implementing the `RenderTarget` trait:

**`SurfaceRenderTarget`** (Component) - Renders to a window surface:
- Color attachment always present
- Optional depth/stencil
- Present mode configuration (VSync, NoVsync, Auto variants)
- Handles surface reconfiguration on resize
- `update(device, surface)` returns a `SurfaceUpdateStatus` (`Ready`, `ReadySuboptimal`, `Skipped`, `Failed`). The `ReadySuboptimal` case lets the renderer keep drawing the current frame while reconfiguring for the next, eliminating black-frame flicker on resize.

**`OffscreenRenderTarget`** (Component) - Renders to textures:
- Optional color, depth/stencil, multisampling
- Configurable texture formats and usages
- Supports resize, clear, resolve operations
- Changes scheduled and applied during `ApplyOffscreenTargetsSystemSet`

**Configuration Types:**
- `RenderTargetColorConfig` - Clear color, multisample settings, format override
- `RenderTargetDepthStencilConfig` - Depth/stencil format, clear values
- `RenderTargetMultisampleConfig` - MSAA sample count

### Pipeline Management

**`RenderPipelineManager`** - Caches one `RenderPipeline` per `(color_format, depth_stencil_format, sample_count)` tuple:
```rust
let pipeline = manager.get(&mut world, &PipelineParameters {
    color_format: Some(format),
    depth_stencil_format: None,
    sample_count: 4,
});
// Or pick parameters from a target automatically:
let pipeline = manager.get_compatible(render_target_source, &mut world);
```

**`GenericRenderPipelineDescriptor`** - Pipeline descriptor without format info, allowing pipeline creation for any render target format at runtime.

**`RenderPipelineResourceProvider`** trait - Abstracts shader module and pipeline layout sources for pipeline creation. The simplest implementation is `DirectRenderPipelineResourceProvider`, which takes asset IDs.

### Bind Group Composition

A system for defining self-contained bind groups — each owning its GPU layout, WGSL shader declarations, and (for runtime layouts) buffer management — then composing them into a final pipeline. Each bind group is an independent module you can swap or reuse without touching the rest of the renderer.

**`BindGroupLayoutProvider`** trait - Defines a bind group layout and an associated WGSL library snippet. Uses `#BIND_GROUP` as a placeholder for the group index.

**`BindGroupLayoutDef`** trait - Compile-time-known layout. Pair with `CachedBindGroupLayout<P>` (resource) and `BindGroupLayoutInitPlugin<P>` for zero-boilerplate static layouts.

**`BindGroupProvider`** trait - Runtime interface providing `&BindGroup` for render passes.

**`SimpleBindGroupLayoutBuilder`** - Builder for runtime bind groups with arbitrary bindings and typed uniforms. Supports `add_entry` for manual bindings (textures, samplers, etc.) and `add_uniform::<Ty>` for type-safe uniforms that auto-generate layout entries and WGSL declarations.

**`SimpleBindGroupProvider`** - Created by binding resources to a `SimpleBindGroupLayoutProvider`. Holds the `BindGroup` and uniform `Buffer`s. Update uniforms at runtime via `set_uniform(&queue, &entry, value)` with type-safe `UniformEntry<Ty>` handles.

**`PipelineLayoutComposer`** - Combines multiple bind group layout providers into a single `PipelineLayout` and a composed `ShaderModule`. Each provider's WGSL library is concatenated with `#BIND_GROUP` substituted for the actual group index. Additional WGSL snippets can be prepended. Results are cached. (Previously used `naga_oil` for module merging; now does plain string concatenation since commit `fd34a17`.)

See `crates/modul_render/DOCS.md` for detailed usage and examples.

### Sequences

A scheduling system for render operations:

**`Sequence`** (Asset) - An ordered list of render operations:
- Built from `OperationBuilder` trait implementations
- Automatically inserts resolve operations when reading previously-written targets
- Lazy initialization on first run

**`OperationBuilder`** trait - Declares which render targets are read/written and produces an `Operation`:
```rust
fn reading(&self) -> Vec<RenderTargetSource>
fn writing(&self) -> Vec<RenderTargetSource>
fn finish(self, world: &World, device: &Device) -> impl Operation + 'static
```

**`Operation`** trait - Executes actual render commands.

**`RunningSequenceQueue`** - Resource holding sequences to execute each frame.

**Built-in Operations:**
- `ClearNext` - Schedule color clear for next pass
- `EmptyPass` - Empty render pass (for resolve)

---

## modul_texture

**Purpose:** Image loading and GPU texture management.

### Image Types

**`Image`** - Raw image data (pixels + dimensions):
```rust
Image::from_path(path)      // Load from file
Image::from_memory(bytes)   // Load from memory
Image::from_dynamic(img)    // From image crate's DynamicImage
```

**`MipMapImage`** - Supports mipmapping:
- `WithImages(Vec<Image>)` - All mipmap levels provided
- `FromLevel(Image, level)` - Engine generates remaining levels

### GPU Resources

**`ViewTexture`** - A GPU texture with its view (wgpu::Texture + TextureView).

**`TextureQueue`** - Deferred texture operations:
```rust
queue.init(id, size, format, ...)  // Create texture
queue.write(id, mipmap_image)      // Upload data
```
Operations are applied during `PreDraw` in `TextureLoadSet`.

### High-Level API

**`TextureLoader`** SystemParam:
```rust
loader.load_texture(path)                    // Load single texture
loader.load_layered_texture(paths)           // Load texture array (same size)
```

### Plugin

`TextureLoadPlugin` - Initializes texture assets and the loading system.

---

## Examples

The `examples/` directory contains:

| Example | Description |
|---------|-------------|
| `window.rs` | Basic window creation |
| `colors.rs` | Color rendering |
| `multi_window.rs` | Multiple window management |
| `triangle.rs` | Full rendering pipeline demo |

### Triangle Example Overview

Demonstrates the complete rendering workflow:
1. Create shader modules (vertex/fragment)
2. Create pipeline layout with bind groups
3. Build `RenderPipelineManager` from generic descriptor
4. Create `Sequence` with triangle drawing operation
5. Queue sequence for execution each frame

---

## Design Patterns

**ECS-Driven** - Heavy use of Bevy's systems, components, resources, and queries throughout.

**Deferred Execution** - Render target changes, texture loads, and sequences are scheduled then applied later in the frame.

**Generic Pipelines** - Pipelines created on-demand for specific render target formats, avoiding premature format decisions.

**Asset-Based** - Shaders, pipelines, textures, and sequences stored as assets with copyable handles.

**Trait-Based Extensibility** - `OperationBuilder`, `RenderPipelineResourceProvider`, `BindGroupLayoutProvider` allow custom implementations.

**Lazy Initialization** - Sequences and pipelines initialized on first use rather than at creation time.
