# modul_asset

Generic asset storage and management system.

## Core Types

### AssetId<T>

A copyable, type-safe handle to an asset:
```rust
let id: AssetId<Texture> = assets.add(texture);
```

Uses a simple usize internally. Handles are stable for the asset's lifetime.

### Assets<T>

Resource holding all assets of a given type:
```rust
let id = assets.add(value);      // Add, get ID
let val = assets.get(id);        // Option<&T>
let val = assets.get_mut(id);    // Option<&mut T>
assets.replace(id, new_val);     // Replace existing
let val = assets.remove(id);     // Remove and return
```

### AssetMap<K, V>

HashMap keyed by `AssetId<K>`. Useful for associating metadata:
```rust
let mut map: AssetMap<Texture, TextureInfo> = AssetMap::new();
map.insert(texture_id, info);
```

## World Extensions

`AssetWorldExt` trait adds methods to `World`:

```rust
world.add_asset::<T>(value)              // Add asset
world.get_asset::<T>(id)                 // Get reference
world.asset_mut::<T, R>(id, |a| ...)     // Mutable access
world.with_asset::<T, R>(id, |a| ...)    // Immutable access
world.asset_scope::<T, R>(|assets| ...)  // Access entire storage
```

## App Extensions

`AssetAppExt` trait:
```rust
app.init_assets::<MyAsset>();  // Initialize storage at startup
```

## Design Notes

- Handles are Copy, avoiding borrow complexity
- Simple counter-based ID allocation
- No reference counting - manual removal required
