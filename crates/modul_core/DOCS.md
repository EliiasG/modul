# modul_core

Foundation crate providing the event loop, window management, and GPU initialization.

> **Best practice:** import `wgpu` and `winit` types via `modul_core::wgpu`
> and `modul_core::winit` rather than adding them to your own `Cargo.toml`.
> modul re-exports them so your code automatically inherits the same pinned
> major version. Mixing modul's wgpu with your own causes confusing
> "expected `wgpu::Device`, found `wgpu::Device`" errors.

## Entry Point

```rust
run_app(graphics_initializer, setup_fn)
```

Creates the winit event loop, initializes GPU resources, runs `PreInit`,
then enters the main loop. `setup_fn` receives a mutable `&mut SubApp` and
runs once before the event loop starts — register systems and plugins here.

## Schedules

| Schedule | When |
|----------|------|
| `PreInit` | Before winit/wgpu are set up — load assets that don't need a Device |
| `Init` | After winit/wgpu are set up — first opportunity to use `RenderContext` |
| `Redraw` | Main loop, on window redraw request |

## Resources

| Resource | Description |
|----------|-------------|
| `RenderContext` | Bundled wgpu `instance`/`adapter`/`device`/`queue`. Take `Res<RenderContext>` in any system that touches wgpu. |
| `SurfaceFormat` | Default surface texture format chosen by the active `GraphicsInitializer` |
| `WindowMap` | `WindowId` → `Entity` mapping |
| `EventBuffer` | winit events accumulated since the last `Redraw` |
| `ShouldExit` | Insert this resource to make the app exit at the end of the current `Redraw` |

## Window Components

| Component | Description |
|-----------|-------------|
| `WindowComponent` | Window + Surface wrapper, spawned for each window |
| `MainWindow` | Marker for the primary (first) window |
| `ImportantWindow` | A draw is performed when this window requests a redraw |
| `UpdatingWindow` | This window requests a redraw every frame |
| `InitialWindowConfig` | Spawn this on an entity to request a new window |

## Graphics Initialization

Implement the `GraphicsInitializer` trait to customize GPU setup, or use
`DefaultGraphicsInitializer` for standard behavior.

`DefaultGraphicsInitializer` exposes:

- `power_preference: PowerPreference`
- `window_attribs: WindowAttributes` — configure the main window
- `required_features: wgpu::Features` — opt into wgpu features beyond the spec minimum
- `required_limits: wgpu::Limits` — opt into higher resource limits

It implements `Default`, so use struct-update syntax to set only the fields
you care about:

```rust
DefaultGraphicsInitializer {
    window_attribs: WindowAttributes::default().with_title("hello"),
    required_features: wgpu::Features::PUSH_CONSTANTS,
    ..Default::default()
}
```

### Surface format selection

The `GraphicsInitializer` trait has a `pick_surface_format(&self, caps)`
method with a default implementation that picks the first sRGB format.
Override it to support HDR or linear-space pipelines:

```rust
impl GraphicsInitializer for MyInitializer {
    fn initialize(self, event_loop: &ActiveEventLoop) -> GraphicsInitializerResult {
        // ... call self.pick_surface_format(&caps) somewhere ...
    }

    fn pick_surface_format(&self, caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
        caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0])
    }
}
```

Custom `initialize` impls are expected to call `self.pick_surface_format(...)`
themselves — the trait method exists as a convention for consistent format
selection across initializers.

## Internal

`WinitApp` handles the winit `ApplicationHandler`, buffering events and
managing the redraw cycle. The field order in `WinitApp` is load-bearing
on Wayland — `app` (containing the `World`) must drop before `_instance`
which must drop before `_main_window`, otherwise the EGL display is torn
down before GPU resources are released and the process segfaults on exit.
Do not reorder these fields. See commit `39a87ee`.
