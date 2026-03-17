# modul_texture

Image loading and GPU texture management.

## Image Types

### Image

Raw image data with dimensions:
```rust
let img = Image::from_path("texture.png")?;
let img = Image::from_memory(bytes)?;
let img = Image::from_dynamic(dynamic_image);
```

### MipMapImage

Mipmap support with two modes:
```rust
// All levels provided
let mip = MipMapImage::WithImages(vec![level0, level1, level2]);

// Generate from base level
let mip = MipMapImage::FromLevel(base_image, 0);
```

Write to GPU:
```rust
mip.write_to_texture(queue, texture, ...);
```

## GPU Resources

### ViewTexture

GPU texture with its view:
```rust
struct ViewTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
```

Stored as an asset via `AssetId<ViewTexture>`.

## Loading System

### TextureQueue

Deferred texture operations (resource):
```rust
texture_queue.init(id, size, format, usage, ...);
texture_queue.write(id, mipmap_image);
```

Operations applied during `PreDraw` in `TextureLoadSet`.

### TextureLoader (SystemParam)

High-level texture loading:
```rust
fn my_system(mut loader: TextureLoader) {
    let id = loader.load_texture("path/to/image.png");
    let id = loader.load_layered_texture(&["layer0.png", "layer1.png"]);
}
```

Layered textures require all images to be the same size.

## Plugin

Add `TextureLoadPlugin` to initialize:
- `Assets<ViewTexture>` storage
- `TextureQueue` resource
- Loading systems in `PreDraw`

## Usage Pattern

```rust
// In setup
let texture_id = loader.load_texture("diffuse.png");

// Later, in render code
let texture = world.get_asset::<ViewTexture>(texture_id);
```
