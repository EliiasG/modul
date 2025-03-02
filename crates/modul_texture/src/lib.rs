// TODO Handle mipmapping

use bevy_app::{App, Plugin};
use bevy_ecs::{prelude::*, system::SystemParam};
use image::{DynamicImage, ImageError, ImageReader};
use modul_asset::{AssetAppExt, AssetId, Assets};
use modul_core::{DeviceRes, QueueRes};
use modul_render::PreDraw;
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    io,
    path::Path,
    slice,
};
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

/// Systems that load textures during [PreDraw], anything that runs in [PreDraw] and needs textures should run after this
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextureLoadSet;

pub struct TextureLoadPlugin;

impl Plugin for TextureLoadPlugin {
    fn build(&self, app: &mut App) {
        app.init_assets::<ViewTexture>();
        app.insert_resource(TextureQueue { queue: Vec::new() });
        app.add_systems(PreDraw, load_textures.in_set(TextureLoadSet));
    }
}

#[derive(Debug)]
pub enum ImageLoadError {
    IOError(io::Error),
    ImageError(ImageError),
}

impl Error for ImageLoadError {}

impl Display for ImageLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ImageLoadError::IOError(e) => write!(f, "Texture load IOError: {}", e),
            ImageLoadError::ImageError(e) => write!(f, "Texture load ImageError: {}", e),
        }
    }
}

impl From<io::Error> for ImageLoadError {
    fn from(value: io::Error) -> Self {
        Self::IOError(value)
    }
}

impl From<ImageError> for ImageLoadError {
    fn from(value: ImageError) -> Self {
        Self::ImageError(value)
    }
}

/// Actual representation of image data, not a GPU resource.  
/// This is mostly used as a layer between image files and [Textures](Texture)
#[derive(Clone)]
pub struct Image {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Image {
    /// Load from file data
    pub fn load_from_data(data: &[u8]) -> Result<Self, ImageLoadError> {
        Ok(image::load_from_memory(data)?.into())
    }
    /// Load from file
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ImageLoadError> {
        Ok(ImageReader::open(path)?.decode()?.into())
    }

    pub fn to_mipmap(self, level_count: usize) -> MipMapImage {
        MipMapImage::from_level(self, level_count)
    }
}

// FIXME maybe don't use image lib publicly, as web should maybe use a different implementation
// or maybe make another method for web...
impl From<DynamicImage> for Image {
    fn from(value: DynamicImage) -> Self {
        Self {
            data: value.to_rgba8().into_vec(),
            width: value.width(),
            height: value.height(),
        }
    }
}

/// A collection of [Images](Image) to be used for mipmap layers
#[derive(Clone)]
pub enum MipMapImage {
    /// All layers of the MipMapImage are described
    WithImages(Vec<Image>),
    /// Only the first level is provided, and the engine is left to generate the rest
    FromLevel(Image, usize),
}

impl MipMapImage {
    /// Makes a new [MipMapImage] from its layers
    /// This means that all levels are provided, use [from_level](MipMapImage::from_level) to have the engine automatically generate levels
    /// ## Panics
    /// If levels is empty
    pub fn with_images(levels: Vec<Image>) -> Self {
        if levels.is_empty() {
            panic!("levels may not be empty");
        }
        Self::WithImages(levels)
    }

    pub fn from_level(base: Image, level: usize) -> Self {
        if level == 0 {
            panic!("MipMapImage must not have 0 levels!")
        }
        Self::FromLevel(base, level)
    }

    #[inline]
    pub fn level_count(&self) -> usize {
        match self {
            MipMapImage::WithImages(img) => img.len(),
            MipMapImage::FromLevel(_, c) => *c,
        }
    }

    /// If called on [FromLevel](MipMapImage::FromLevel) will only return the base image
    #[inline]
    pub fn levels(&self) -> &[Image] {
        match self {
            MipMapImage::WithImages(levels) => levels,
            MipMapImage::FromLevel(img, _) => slice::from_ref(img),
        }
    }

    #[inline]
    pub fn to_levels(self) -> Vec<Image> {
        match self {
            MipMapImage::WithImages(levels) => levels,
            MipMapImage::FromLevel(level, _) => vec![level],
        }
    }

    /// If called on [FromLevel](MipMapImage::FromLevel) will only return the base image size
    pub fn sizes(&self) -> Vec<(u32, u32)> {
        self.levels()
            .iter()
            .map(|img| (img.width, img.height))
            .collect()
    }

    /// Directly writes to a texture, for most cases [TextureLoader] or [TextureQueue] should be sufficient
    pub fn write_to_texture(&self, queue: &Queue, origin: Origin3d, texture: &Texture) {
        for (mip_level, image) in self.levels().into_iter().enumerate() {
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture,
                    origin,
                    mip_level: mip_level as u32,
                    aspect: TextureAspect::All,
                },
                &image.data,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * image.width),
                    rows_per_image: Some(image.height),
                },
                Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

impl From<Image> for MipMapImage {
    fn from(value: Image) -> Self {
        Self::from_level(value, 1)
    }
}

/// A texture with a view used for reading in shaders.  
/// If you want to draw to a texture consider a [RenderTarget](modul_render::RenderTarget)
pub struct ViewTexture {
    pub texture: Texture,
    pub view: TextureView,
}

pub enum LayeredTextureError {
    /// Returned if a layered image was attempted, but there are no layers
    NoLayers,
    /// Returned if not all layers share the same size for every mipmap level
    InvalidLayer,
}

/// used to put textures in assets, if the goal is to just load a texture consider [TextureLoader]
#[derive(Resource)]
pub struct TextureQueue {
    queue: Vec<TextureOperation>,
}

impl TextureQueue {
    /// inits a texture on the given asset, discards the current texture if it already exists
    pub fn init(
        &mut self,
        asset_id: AssetId<ViewTexture>,
        size: (u32, u32),
        usage: TextureUsages,
        mip_count: u32,
        layers: Option<u32>,
    ) {
        self.queue
            .push(TextureOperation::InitTexture(TextureInitInfo {
                asset_id,
                size,
                usage,
                mip_count,
                layers,
            }));
    }

    /// writes a 2d image to the texture at the given asset, if it does not exist a panic will occur
    pub fn write(
        &mut self,
        image: impl Into<MipMapImage>,
        asset_id: AssetId<ViewTexture>,
        origin: Origin3d,
    ) {
        self.queue
            .push(TextureOperation::WriteTexture(TextureWriteInfo {
                image: image.into(),
                asset_id,
                origin,
            }));
    }
}

#[derive(SystemParam)]
pub struct TextureLoader<'w> {
    texture_queue: ResMut<'w, TextureQueue>,
    texture_assets: ResMut<'w, Assets<ViewTexture>>,
}

impl TextureLoader<'_> {
    /// loads a texture
    pub fn load_texture(&mut self, image: impl Into<MipMapImage>) -> AssetId<ViewTexture> {
        let image = image.into();
        let asset_id = self.texture_assets.add_empty();
        self.texture_queue.init(
            asset_id,
            image.sizes()[0],
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            1,
            None,
        );
        self.texture_queue.write(image, asset_id, Origin3d::ZERO);
        asset_id
    }

    /// loads a layered image, all layers must be same size
    pub fn load_layered_texture(
        &mut self,
        layers: Vec<MipMapImage>,
    ) -> Result<AssetId<ViewTexture>, LayeredTextureError> {
        // checking to not allocate asset in case of error
        if let Some(err) = validate_layers(&layers) {
            return Err(err);
        }
        let asset_id = self.texture_assets.add_empty();
        self.texture_queue.init(
            asset_id,
            layers[0].sizes()[0],
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            layers.len() as u32,
            Some(layers.len() as u32),
        );
        for (layer, mip_image) in layers.into_iter().enumerate() {
            self.texture_queue.write(
                mip_image,
                asset_id,
                Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
            );
        }
        Ok(asset_id)
    }
}

fn validate_layers(images: &Vec<MipMapImage>) -> Option<LayeredTextureError> {
    if images.is_empty() {
        return Some(LayeredTextureError::NoLayers);
    }
    let first_size = images[0].sizes();
    if images[1..].iter().all(|img| img.sizes() == first_size) {
        None
    } else {
        Some(LayeredTextureError::InvalidLayer)
    }
}

enum TextureOperation {
    WriteTexture(TextureWriteInfo),
    InitTexture(TextureInitInfo),
}

struct TextureWriteInfo {
    image: MipMapImage,
    asset_id: AssetId<ViewTexture>,
    origin: Origin3d,
}

struct TextureInitInfo {
    asset_id: AssetId<ViewTexture>,
    size: (u32, u32),
    usage: TextureUsages,
    mip_count: u32,
    layers: Option<u32>,
}

fn load_textures(
    mut texture_queue: ResMut<TextureQueue>,
    mut texture_assets: ResMut<Assets<ViewTexture>>,
    device: Res<DeviceRes>,
    queue: Res<QueueRes>,
) {
    for op in texture_queue.queue.drain(..) {
        match op {
            TextureOperation::InitTexture(info) => {
                init_texture(info, &mut texture_assets, &device.0)
            }
            TextureOperation::WriteTexture(info) => write_texture(info, &texture_assets, &queue.0),
        }
    }
}

fn write_texture(info: TextureWriteInfo, texture_assets: &Assets<ViewTexture>, queue: &Queue) {
    info.image.write_to_texture(
        queue,
        info.origin,
        &texture_assets.get(info.asset_id).unwrap().texture,
    );
}

fn init_texture(info: TextureInitInfo, texture_assets: &mut Assets<ViewTexture>, device: &Device) {
    let texture = device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: info.size.0,
            height: info.size.1,
            depth_or_array_layers: info.layers.unwrap_or(1),
        },
        mip_level_count: info.mip_count,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: info.usage,
        view_formats: &[],
    });
    texture_assets.replace(
        info.asset_id,
        ViewTexture {
            view: texture.create_view(&TextureViewDescriptor::default()),
            texture,
        },
    );
}
