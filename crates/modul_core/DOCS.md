# modul_core

Foundation crate providing the event loop, window management, and GPU initialization.

## Entry Point

```rust
run_app(graphics_initializer, setup_fn)
```

Creates the winit event loop, initializes GPU resources, runs setup, then enters the main loop.

## Schedules

| Schedule | When |
|----------|------|
| `PreInit` | Before graphics setup |
| `Init` | After graphics setup |
| `Redraw` | Main loop, on window redraw request |

## Resources

| Resource | Description |
|----------|-------------|
| `InstanceRes` | wgpu Instance |
| `DeviceRes` | wgpu Device |
| `QueueRes` | wgpu Queue |
| `AdapterRes` | wgpu Adapter |
| `SurfaceFormat` | Default texture format |
| `WindowMap` | WindowId → Entity mapping |
| `EventBuffer` | Accumulated events |
| `ShouldExit` | Exit signal |

## Window Components

| Component | Description |
|-----------|-------------|
| `WindowComponent` | Window + Surface wrapper |
| `MainWindow` | Primary window marker |
| `ImportantWindow` | App exits when all close |
| `UpdatingWindow` | Triggers redraws |
| `InitialWindowConfig` | New window config |

## Graphics Initialization

Implement `GraphicsInitializer` trait to customize GPU setup. Use `DefaultGraphicsInitializer` for standard behavior.

## Internal

`WinitApp` handles the winit `ApplicationHandler`, buffering events and managing the redraw cycle.
