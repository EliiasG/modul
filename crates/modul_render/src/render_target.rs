use bevy_ecs::component::Component;
use log::warn;
use wgpu::{
    Color, CommandEncoder, Device, Extent3d, LoadOp, Operations, PresentMode, RenderPass,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp,
    Surface, SurfaceCapabilities, SurfaceConfiguration, SurfaceError, SurfaceTexture, Texture,
    TextureDescriptor, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
};
// almost 1000 lines of BS
// brace yourself

#[derive(Clone, PartialEq)]
pub struct RenderTargetDepthStencilConfig {
    /// The clear depth of the render target
    pub clear_depth: f32,
    /// The clear stencil of the render target
    pub clear_stencil: u32,
    /// The usages of the depth/stencil texture, [RENDER_ATTACHMENT](TextureUsages::RENDER_ATTACHMENT) always set
    pub usages: TextureUsages,
    /// The format of the depth/stencil texture
    pub format: TextureFormat,
}

impl Default for RenderTargetDepthStencilConfig {
    fn default() -> Self {
        RenderTargetDepthStencilConfig {
            clear_depth: 1.0,
            clear_stencil: 0,
            usages: TextureUsages::RENDER_ATTACHMENT,
            format: TextureFormat::Depth24PlusStencil8,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RenderTargetMultisampleConfig {
    /// sample count of the internal Texture
    pub sample_count: u32,
}

impl Default for RenderTargetMultisampleConfig {
    #[inline]
    fn default() -> Self {
        RenderTargetMultisampleConfig { sample_count: 4 }
    }
}

#[derive(Clone, PartialEq)]
pub struct RenderTargetColorConfig {
    /// If Some the texture will be multisample with the given sample count
    pub multisample_config: Option<RenderTargetMultisampleConfig>,
    /// The clear color of the render target
    pub clear_color: Color,
    /// The usages of the main texture, [RENDER_ATTACHMENT](TextureUsages::RENDER_ATTACHMENT) always set
    pub usages: TextureUsages,
    /// The format of the color texture, if none [OffscreenRenderTarget] will default to Rgba8UnormSrgb, while surfaces will choose the preferred Srgb format
    pub format_override: Option<TextureFormat>,
}

impl Default for RenderTargetColorConfig {
    #[inline]
    fn default() -> Self {
        RenderTargetColorConfig {
            clear_color: Color::BLACK,
            usages: TextureUsages::RENDER_ATTACHMENT,
            format_override: None,
            multisample_config: None,
        }
    }
}

/// A generic type that can be rendered to.
/// All [RenderTarget]s should support Color, Depth/Stencil and Multisampling, however it's up to the specific implementation which are required.
/// [RenderTarget]s also have functionality schedule modifications, it is up to the implementations to apply them.
/// The built-in [RenderTarget]s are [OffscreenRenderTarget], where all buffers are optional, and [SurfaceRenderTarget] that requires a color buffer.
/// I cannot imagine needing any other implementations than the two built-ins.
pub trait RenderTarget {
    /// The size of the textures, might be 0 on a [SurfaceRenderTarget] if not initialized
    fn size(&self) -> (u32, u32);
    /// Sample count of the internal Texture, will be 1 if not multisampled
    fn sample_count(&self) -> u32;
    /// The clear color of the render target, if no color buffer is used this will return None
    fn clear_color(&self) -> Option<Color>;
    /// The clear depth of the render target, if no depth/stencil buffer is used this will return None
    fn clear_depth(&self) -> Option<f32>;
    /// The clear stencil of the render target, if no depth/stencil buffer is used this will return None
    fn clear_stencil(&self) -> Option<u32>;
    /// The primary texture of the RenderTarget, might be changed when the RenderTarget is resized (and possibly in other situations)
    fn texture(&self) -> Option<&Texture>;
    /// The primary texture view of the RenderTarget, might be changed when the RenderTarget is resized (and possibly in other situations)
    fn texture_view(&self) -> Option<&TextureView>;
    /// The depth/stencil texture of the RenderTarget, might be changed when the RenderTarget is resized (and possibly in other situations)
    fn depth_stencil(&self) -> Option<&Texture>;
    /// The depth/stencil texture view of the RenderTarget, might be changed when the RenderTarget is resized (and possibly in other situations)
    fn depth_stencil_view(&self) -> Option<&TextureView>;
    /// The current color config, the scheduled config will be applied based on the implementation
    fn current_color_config(&self) -> Option<&RenderTargetColorConfig>;
    /// An immutable reference to the scheduled color config.
    fn scheduled_color_config(&self) -> Option<&RenderTargetColorConfig>;
    /// Get a mutable reference to the scheduled color config, is there are no scheduled changes a copy of the current config will be scheduled.
    /// Will only be [None] if there is no color config at all.
    /// Will always be [Some] on a [SurfaceRenderTarget].
    fn scheduled_color_config_mut(&mut self) -> Option<&mut RenderTargetColorConfig>;
    /// Sets the scheduled color config, this config will be applied based on the implementor
    fn set_scheduled_color_config(&mut self, config: RenderTargetColorConfig);
    /// The current depth/stencil config, the scheduled config will be applied based on the implementation
    fn current_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig>;
    /// An immutable reference to the scheduled depth/stencil config.
    fn scheduled_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig>;
    /// Get a mutable reference to the scheduled depth/stencil config, is there are no scheduled changes a copy of the current config will be scheduled.
    /// Will only be [None] if there is no depth/stencil config at all.
    fn scheduled_depth_stencil_config_mut(&mut self)
        -> Option<&mut RenderTargetDepthStencilConfig>;
    /// Sets the scheduled depth/stencil config, this config will be applied based on the implementor.
    fn set_scheduled_depth_stencil_config(&mut self, config: RenderTargetDepthStencilConfig);
    /// Set the scheduled clear color of the render target, if no color buffer is used this will do nothing.
    fn set_clear_color(&mut self, color: Color);
    /// Set the scheduled clear depth of the render target, if no depth/stencil buffer is used this will do nothing.
    fn set_clear_depth(&mut self, depth: f32);
    /// Set the scheduled clear stencil of the render target, if no depth/stencil buffer is used this will do nothing.
    fn set_clear_stencil(&mut self, stencil: u32);
    /// The next [RenderPass] created will clear the main texture.
    /// Note that if the render target is multisampled the multisampled texture will be cleared, and the main texture will not be cleared before the next resolve.
    fn schedule_clear_color(&mut self);
    /// The next [RenderPass] created will clear the depth.
    fn schedule_clear_depth(&mut self);
    /// The next [RenderPass] created will clear the depth.
    fn schedule_clear_stencil(&mut self);
    /// Next [RenderPass] created will be resolving, when using [Sequences](super::Sequence) this a called automatically
    fn schedule_resolve(&mut self);
    /// Begins a render pass, the pass will be resolving if [schedule_resolve](RenderTarget::schedule_resolve) was called after the last call to this method
    fn begin_ending_pass<'a>(
        &mut self,
        command_encoder: &'a mut CommandEncoder,
    ) -> Option<RenderPass<'a>>;
    /// Begins a render pass, the pass will always be resolving
    fn begin_resolving_pass<'a>(
        &mut self,
        command_encoder: &'a mut CommandEncoder,
    ) -> Option<RenderPass<'a>>;
    /// Begins a render pass, the pass will not be resolving, this should be used for every pass except for the last if a [Operation](super::Operation) needs multiple passes
    fn begin_non_resolving_pass<'a>(
        &mut self,
        command_encoder: &'a mut CommandEncoder,
    ) -> Option<RenderPass<'a>>;
}

// This is where the somewhat good code ends

/// Some of the functions of [RenderTarget] can be implemented with the others.
/// I could have made an extension trait, but [OffscreenRenderTarget] and [SurfaceRenderTarget] are probably the only types to implement [RenderTarget].
/// For that reason I would rather have the library be cleaner on the outside.
trait RenderTargetImpl {
    /// Required because textures might not be available.
    fn size(&self) -> (u32, u32);
    fn texture(&self) -> Option<&Texture>;
    fn texture_view(&self) -> Option<&TextureView>;
    fn multisampled_view(&self) -> Option<&TextureView>;
    fn depth_stencil(&self) -> Option<&Texture>;
    fn depth_stencil_view(&self) -> Option<&TextureView>;
    fn current_color_config(&self) -> Option<&RenderTargetColorConfig>;
    fn scheduled_color_config(&self) -> Option<&RenderTargetColorConfig>;
    fn scheduled_color_config_mut(&mut self) -> Option<&mut RenderTargetColorConfig>;
    fn set_scheduled_color_config(&mut self, config: RenderTargetColorConfig);
    fn current_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig>;
    fn scheduled_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig>;
    fn scheduled_depth_stencil_config_mut(&mut self)
        -> Option<&mut RenderTargetDepthStencilConfig>;
    fn set_scheduled_depth_stencil_config(&mut self, config: RenderTargetDepthStencilConfig);
    fn schedule_clear_color(&mut self);
    fn schedule_clear_depth(&mut self);
    fn schedule_clear_stencil(&mut self);
    fn schedule_resolve(&mut self);
    /// Required for making renderpasses
    fn scheduled_resolve(&self) -> bool;
    /// Required for making renderpasses, return color, depth, stencil
    fn clearing(&self) -> (bool, bool, bool);
    /// Tells the implementation that a pass was created and the next pass shouldn't clear
    fn pass_created(&mut self);
    /// called when a finishing pass is created
    fn unschedule_resolve(&mut self);
}

impl<T: RenderTargetImpl> RenderTarget for T {
    fn size(&self) -> (u32, u32) {
        self.size()
    }

    fn sample_count(&self) -> u32 {
        self.current_color_config()
            .map(|c| {
                c.multisample_config
                    .as_ref()
                    .map(|c| c.sample_count)
                    .unwrap_or(1)
            })
            .unwrap_or(1)
    }

    fn clear_color(&self) -> Option<Color> {
        self.current_color_config().map(|c| c.clear_color)
    }

    fn clear_depth(&self) -> Option<f32> {
        self.current_depth_stencil_config().map(|c| c.clear_depth)
    }

    fn clear_stencil(&self) -> Option<u32> {
        self.current_depth_stencil_config().map(|c| c.clear_stencil)
    }

    fn texture(&self) -> Option<&Texture> {
        self.texture()
    }

    fn texture_view(&self) -> Option<&TextureView> {
        self.texture_view()
    }

    fn depth_stencil(&self) -> Option<&Texture> {
        self.depth_stencil()
    }

    fn depth_stencil_view(&self) -> Option<&TextureView> {
        self.depth_stencil_view()
    }

    fn current_color_config(&self) -> Option<&RenderTargetColorConfig> {
        self.current_color_config()
    }

    fn scheduled_color_config(&self) -> Option<&RenderTargetColorConfig> {
        self.scheduled_color_config()
    }

    fn scheduled_color_config_mut(&mut self) -> Option<&mut RenderTargetColorConfig> {
        self.scheduled_color_config_mut()
    }

    fn set_scheduled_color_config(&mut self, config: RenderTargetColorConfig) {
        self.set_scheduled_color_config(config);
    }

    fn current_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig> {
        self.current_depth_stencil_config()
    }

    fn scheduled_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig> {
        self.current_depth_stencil_config()
    }

    fn scheduled_depth_stencil_config_mut(
        &mut self,
    ) -> Option<&mut RenderTargetDepthStencilConfig> {
        self.scheduled_depth_stencil_config_mut()
    }

    fn set_scheduled_depth_stencil_config(&mut self, config: RenderTargetDepthStencilConfig) {
        self.set_scheduled_depth_stencil_config(config);
    }

    fn set_clear_color(&mut self, color: Color) {
        self.scheduled_color_config_mut()
            .map(|c| c.clear_color = color);
    }

    fn set_clear_depth(&mut self, depth: f32) {
        self.scheduled_depth_stencil_config_mut()
            .map(|c| c.clear_depth = depth);
    }

    fn set_clear_stencil(&mut self, stencil: u32) {
        self.scheduled_depth_stencil_config_mut()
            .map(|c| c.clear_stencil = stencil);
    }

    fn schedule_clear_color(&mut self) {
        self.schedule_clear_color();
    }

    fn schedule_clear_depth(&mut self) {
        self.schedule_clear_depth();
    }

    fn schedule_clear_stencil(&mut self) {
        self.schedule_clear_stencil();
    }

    fn schedule_resolve(&mut self) {
        self.schedule_resolve();
    }

    fn begin_ending_pass<'a>(
        &mut self,
        command_encoder: &'a mut CommandEncoder,
    ) -> Option<RenderPass<'a>> {
        let res = create_pass(self, command_encoder, self.scheduled_resolve());
        self.unschedule_resolve();
        res
    }

    fn begin_resolving_pass<'a>(
        &mut self,
        command_encoder: &'a mut CommandEncoder,
    ) -> Option<RenderPass<'a>> {
        create_pass(self, command_encoder, true)
    }

    fn begin_non_resolving_pass<'a>(
        &mut self,
        command_encoder: &'a mut CommandEncoder,
    ) -> Option<RenderPass<'a>> {
        create_pass(self, command_encoder, false)
    }
}

fn create_pass<'a>(
    target: &mut impl RenderTargetImpl,
    command_encoder: &'a mut CommandEncoder,
    resolve: bool,
) -> Option<RenderPass<'a>> {
    let (clear_color, clear_depth, clear_stencil) = target.clearing();
    target.pass_created();
    if target.texture_view().is_none() && target.depth_stencil_view().is_none() {
        return None;
    }
    Some(command_encoder.begin_render_pass(&RenderPassDescriptor {
        label: None,
        color_attachments: &[target.texture_view().map(|view| {
            let multisample = target.multisampled_view();
            RenderPassColorAttachment {
                // if multisample is available do it, else use the regular view
                view: multisample.unwrap_or(view),
                // set resolve target if multisampling and should resolve
                resolve_target: Some(view).filter(|_| multisample.is_some() && resolve),
                ops: Operations {
                    load: if clear_color {
                        LoadOp::Clear(
                            target
                                .current_color_config()
                                .as_ref()
                                .expect("texture but no color config")
                                .clear_color,
                        )
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                },
            }
        })],
        // maybe fix DRY
        depth_stencil_attachment: target.depth_stencil_view().map(|view| {
            RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(Operations {
                    load: if clear_depth {
                        LoadOp::Clear(
                            target
                                .current_depth_stencil_config()
                                .as_ref()
                                .expect("texture but no depth/stencil config")
                                .clear_depth,
                        )
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: if clear_stencil {
                        LoadOp::Clear(
                            target
                                .current_depth_stencil_config()
                                .as_ref()
                                .expect("texture but no depth/stencil config")
                                .clear_stencil,
                        )
                    } else {
                        LoadOp::Load
                    },
                    store: StoreOp::Store,
                }),
            }
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
    }))
}

#[derive(Clone)]
pub struct OffscreenRenderTargetConfig {
    /// The size of the textures
    pub size: (u32, u32),
    /// The color config of the texture, if None the texture will not have a color buffer
    pub color_config: Option<RenderTargetColorConfig>,
    /// The depth/stencil config of the texture, if None the texture will not have a depth/stencil buffer
    pub depth_stencil_config: Option<RenderTargetDepthStencilConfig>,
}

impl Default for OffscreenRenderTargetConfig {
    fn default() -> Self {
        Self {
            size: (1, 1),
            depth_stencil_config: Some(Default::default()),
            color_config: Some(Default::default()),
        }
    }
}

#[derive(Component)]
pub struct OffscreenRenderTarget {
    current_config: Option<OffscreenRenderTargetConfig>,
    scheduled_config: Option<OffscreenRenderTargetConfig>,

    color_texture: Option<(Texture, TextureView)>,
    multisampled_texture: Option<(Texture, TextureView)>,
    depth_stencil_texture: Option<(Texture, TextureView)>,

    resolve_scheduled: bool,
    clear_color_scheduled: bool,
    clear_depth_scheduled: bool,
    clear_stencil_scheduled: bool,
}

impl OffscreenRenderTarget {
    /// Creates a new [OffscreenRenderTarget] with a scheduled config.
    pub fn new(config: OffscreenRenderTargetConfig) -> Self {
        Self {
            current_config: None,
            scheduled_config: Some(config),
            color_texture: None,
            multisampled_texture: None,
            depth_stencil_texture: None,
            resolve_scheduled: false,
            clear_color_scheduled: false,
            clear_depth_scheduled: false,
            clear_stencil_scheduled: false,
        }
    }

    /// The scheduled size of the [OffscreenRenderTarget], will be [None] if resizing is not scheduled
    pub fn scheduled_size(&self) -> Option<(u32, u32)> {
        self.scheduled_config.as_ref().map(|c| c.size)
    }

    /// Schedule a resizing of the [OffscreenRenderTarget]
    pub fn resize(&mut self, size: (u32, u32)) {
        self.scheduled_config_mut().size = size;
    }

    /// Remove the color texture when changes are applied
    pub fn remove_color(&mut self) {
        self.scheduled_config
            .as_mut()
            .map(|c| c.color_config.take());
    }

    /// Remove the depth/stencil texture when changes are applied
    pub fn remove_depth_stencil(&mut self) {
        self.scheduled_config
            .as_mut()
            .map(|c| c.depth_stencil_config.take());
    }

    /// Applies the scheduled changes, this might replace the textures and thereby clear them
    pub fn apply_changes(&mut self, device: &Device) {
        let changes = self.changes();
        if changes == (false, false, false) {
            return;
        }
        let (color_changed, multisample_changed, depth_stencil_changed) = changes;
        if let Some(cfg) = self.scheduled_config.take() {
            self.current_config = Some(cfg);
        }
        let (width, height) = <Self as RenderTarget>::size(self);
        let mut desc = texture_descriptor(width, height);
        if color_changed {
            let mut mt = self.multisampled_texture.take();
            // funky map abuse
            self.color_texture = <Self as RenderTarget>::current_color_config(self).map(|c| {
                desc.format = c.format_override.unwrap_or(TextureFormat::Rgba8UnormSrgb);
                if multisample_changed {
                    mt = c.multisample_config.as_ref().map(|mc| {
                        desc.sample_count = mc.sample_count;
                        with_view(device.create_texture(&desc))
                    });
                }
                desc.sample_count = 1;
                // setting here because multisampled is only RENDER_ATTACHMENT
                desc.usage = c.usages | TextureUsages::RENDER_ATTACHMENT;
                with_view(device.create_texture(&desc))
            });
            self.multisampled_texture = mt;
        }

        if depth_stencil_changed {
            self.depth_stencil_texture = <Self as RenderTarget>::current_depth_stencil_config(self)
                .map(|c| {
                    desc.usage = c.usages | TextureUsages::RENDER_ATTACHMENT;
                    desc.format = c.format;
                    with_view(device.create_texture(&desc))
                })
        }
    }

    fn changes(&self) -> (bool, bool, bool) {
        if self.current_config.is_none() {
            return (true, true, true);
        }
        if self.scheduled_config.is_none() {
            return (false, false, false);
        }
        let cur = self.current_config.as_ref().unwrap();
        let new = self.scheduled_config.as_ref().unwrap();
        if cur.size != new.size {
            return (true, true, true);
        }
        (
            cur.color_config != new.color_config,
            cur.color_config.as_ref().map(|c| &c.multisample_config)
                != new.color_config.as_ref().map(|c| &c.multisample_config),
            cur.depth_stencil_config != new.depth_stencil_config,
        )
    }

    /// Helper for scheduling changes
    fn scheduled_config_mut(&mut self) -> &mut OffscreenRenderTargetConfig {
        self.scheduled_config.get_or_insert_with(|| {
            self.current_config
                .clone()
                .expect("no scheduled or current config")
        })
    }
}

impl RenderTargetImpl for OffscreenRenderTarget {
    fn size(&self) -> (u32, u32) {
        self.current_config
            .as_ref()
            .unwrap_or(
                self.scheduled_config
                    .as_ref()
                    .expect("no scheduled or current config"),
            )
            .size
    }

    fn texture(&self) -> Option<&Texture> {
        self.color_texture.as_ref().map(|(tex, _)| tex)
    }

    fn texture_view(&self) -> Option<&TextureView> {
        self.multisampled_texture.as_ref().map(|(_, view)| view)
    }

    fn multisampled_view(&self) -> Option<&TextureView> {
        self.multisampled_texture.as_ref().map(|(_, view)| view)
    }

    fn depth_stencil(&self) -> Option<&Texture> {
        self.depth_stencil_texture.as_ref().map(|(tex, _)| tex)
    }

    fn depth_stencil_view(&self) -> Option<&TextureView> {
        self.depth_stencil_texture.as_ref().map(|(_, view)| view)
    }

    fn current_color_config(&self) -> Option<&RenderTargetColorConfig> {
        self.current_config
            .as_ref()
            .map(|c| c.color_config.as_ref())?
    }

    fn scheduled_color_config(&self) -> Option<&RenderTargetColorConfig> {
        self.scheduled_config
            .as_ref()
            .map(|c| c.color_config.as_ref())?
    }

    fn scheduled_color_config_mut(&mut self) -> Option<&mut RenderTargetColorConfig> {
        self.scheduled_config_mut().color_config.as_mut()
    }

    fn set_scheduled_color_config(&mut self, config: RenderTargetColorConfig) {
        self.scheduled_config_mut().color_config = Some(config);
    }

    fn current_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig> {
        self.current_config
            .as_ref()
            .map(|c| c.depth_stencil_config.as_ref())?
    }

    fn scheduled_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig> {
        self.scheduled_config
            .as_ref()
            .map(|c| c.depth_stencil_config.as_ref())?
    }

    fn scheduled_depth_stencil_config_mut(
        &mut self,
    ) -> Option<&mut RenderTargetDepthStencilConfig> {
        self.scheduled_config_mut().depth_stencil_config.as_mut()
    }

    fn set_scheduled_depth_stencil_config(&mut self, config: RenderTargetDepthStencilConfig) {
        self.scheduled_config_mut().depth_stencil_config = Some(config);
    }

    fn schedule_clear_color(&mut self) {
        self.clear_color_scheduled = true;
    }

    fn schedule_clear_depth(&mut self) {
        self.clear_depth_scheduled = true;
    }

    fn schedule_clear_stencil(&mut self) {
        self.clear_stencil_scheduled = true;
    }

    fn schedule_resolve(&mut self) {
        self.resolve_scheduled = true;
    }

    fn scheduled_resolve(&self) -> bool {
        self.resolve_scheduled
    }

    fn clearing(&self) -> (bool, bool, bool) {
        (
            self.clear_color_scheduled,
            self.clear_depth_scheduled,
            self.clear_stencil_scheduled,
        )
    }

    fn pass_created(&mut self) {
        self.clear_color_scheduled = false;
        self.clear_depth_scheduled = false;
        self.clear_stencil_scheduled = false;
    }

    fn unschedule_resolve(&mut self) {
        self.resolve_scheduled = false;
    }
}

#[derive(Clone)]
pub struct SurfaceRenderTargetConfig {
    /// Color config of the surface, not optional, as a surface always has a color component
    pub color_config: RenderTargetColorConfig,
    /// The depth/stencil config of the texture, if None the texture will not have a depth/stencil buffer
    pub depth_stencil_config: Option<RenderTargetDepthStencilConfig>,
    /// The [desired_maximum_frame_latency](SurfaceConfiguration::desired_maximum_frame_latency) of the surface
    pub desired_maximum_frame_latency: u32,
    /// The preferred [PresentMode], if not available the engine will attempt to use the set [Self::backup_present_mode].
    pub present_mode: PresentMode,
    /// The [PresentMode] to be used if the desired is unavailable, if the desired is unavailable and this is None/unavailable the program may crash.
    /// This is optional because some [PresentMode]s are always available
    pub backup_present_mode: Option<PresentMode>,
}

impl Default for SurfaceRenderTargetConfig {
    fn default() -> Self {
        Self {
            color_config: Default::default(),
            depth_stencil_config: None,
            desired_maximum_frame_latency: 2,
            present_mode: PresentMode::AutoVsync,
            backup_present_mode: None,
        }
    }
}

#[derive(Component)]
pub struct SurfaceRenderTarget {
    current_config: Option<SurfaceRenderTargetConfig>,
    scheduled_config: Option<SurfaceRenderTargetConfig>,
    size: (u32, u32),

    surface_capabilities: Option<SurfaceCapabilities>,
    preferred_format: Option<TextureFormat>,
    color_texture: Option<(SurfaceTexture, TextureView)>,
    multisampled_texture: Option<(Texture, TextureView)>,
    depth_stencil_texture: Option<(Texture, TextureView)>,

    resized: bool,
    resolve_scheduled: bool,
    clear_color_scheduled: bool,
    clear_depth_scheduled: bool,
    clear_stencil_scheduled: bool,
}

impl SurfaceRenderTarget {
    pub fn new(config: SurfaceRenderTargetConfig) -> Self {
        Self {
            current_config: None,
            scheduled_config: Some(config),
            preferred_format: None,
            surface_capabilities: None,
            size: (0, 0),
            color_texture: None,
            multisampled_texture: None,
            depth_stencil_texture: None,
            resized: false,
            resolve_scheduled: false,
            clear_color_scheduled: false,
            clear_depth_scheduled: false,
            clear_stencil_scheduled: false,
        }
    }

    /// The desired max frame latency of the [SurfaceConfiguration]
    pub fn max_frame_latency(&self) -> u32 {
        self.current_or_scheduled_config()
            .desired_maximum_frame_latency
    }

    /// The [PresentMode] of the [SurfaceConfiguration]
    pub fn present_mode(&self) -> PresentMode {
        self.current_or_scheduled_config().present_mode
    }

    /// The backup [PresentMode] used if the primary isn't available for the [SurfaceConfiguration]
    pub fn backup_present_mode(&self) -> Option<PresentMode> {
        self.current_or_scheduled_config().backup_present_mode
    }

    /// Sets the scheduled desired max frame latency of the [SurfaceConfiguration]
    pub fn set_max_frame_latency(&mut self, max_frame_latency: u32) {
        self.scheduled_config_mut().desired_maximum_frame_latency = max_frame_latency;
    }

    /// Sets the scheduled present mode of the [SurfaceConfiguration]
    pub fn set_present_mode(&mut self, present_mode: PresentMode) {
        self.scheduled_config_mut().present_mode = present_mode;
    }

    /// Sets scheduled backup for the present mode of the [SurfaceConfiguration]
    pub fn set_backup_present_mode(&mut self, backup_present_mode: Option<PresentMode>) {
        self.scheduled_config_mut().backup_present_mode = backup_present_mode;
    }

    /// Remove the depth/stencil texture when changes are applied
    pub fn remove_depth_stencil(&mut self) {
        self.scheduled_config
            .as_mut()
            .map(|c| c.depth_stencil_config.take());
    }

    pub fn init(
        &mut self,
        preferred_surface_format: TextureFormat,
        capabilities: SurfaceCapabilities,
    ) {
        self.preferred_format = Some(preferred_surface_format);
        self.surface_capabilities = Some(capabilities);
    }

    /// Applies the scheduled changes, and updates [SurfaceTexture] this might replace the textures and thereby clear them
    /// Returns whether the window should request a redraw
    pub fn update(&mut self, device: &Device, surface: &Surface) -> Result<(), SurfaceError> {
        // yuck, maybe rewrite in the future?
        // probably not happening
        let (Some(preferred_format), Some(caps)) =
            (&self.preferred_format, &self.surface_capabilities)
        else {
            warn!("Tried to update uninitialized SurfaceRenderTarget");
            // ok because it's a programmer error and not a surface err
            return Ok(());
        };
        let (color_changed, multisampled_changed, depth_stencil_changed) = self.changes();
        if let Some(cfg) = self.scheduled_config.take() {
            self.current_config = Some(cfg);
        }
        let cfg = self.current_config.as_ref().unwrap();
        let surface_cfg = SurfaceConfiguration {
            usage: cfg.color_config.usages | TextureUsages::RENDER_ATTACHMENT,
            format: cfg
                .color_config
                .format_override
                .unwrap_or_else(|| *preferred_format),
            width: self.size.0,
            height: self.size.1,
            present_mode: if cfg.present_mode == PresentMode::AutoVsync
                || cfg.present_mode == PresentMode::AutoNoVsync
                || caps.present_modes.contains(&cfg.present_mode)
            {
                cfg.present_mode
            } else {
                cfg.backup_present_mode
                    .expect("present mode not available, and backup not set")
            },
            desired_maximum_frame_latency: cfg.desired_maximum_frame_latency,
            alpha_mode: Default::default(),
            view_formats: Vec::new(),
        };
        if color_changed || self.resized {
            surface.configure(&device, &surface_cfg);
            if multisampled_changed || self.resized {
                self.multisampled_texture = cfg.color_config.multisample_config.as_ref().map(|m| {
                    let mut desc = texture_descriptor(self.size.0, self.size.1);
                    desc.format = surface_cfg.format;
                    desc.sample_count = m.sample_count;
                    with_view(device.create_texture(&desc))
                });
            }
        }
        match surface.get_current_texture() {
            Ok(t) => {
                let view = t.texture.create_view(&TextureViewDescriptor::default());
                self.color_texture = Some((t, view));
            }
            Err(e) => {
                self.color_texture = None;
                if let SurfaceError::Lost | SurfaceError::Outdated = e {
                    surface.configure(device, &surface_cfg);
                }
                return Err(e);
            }
        };
        if depth_stencil_changed || self.resized {
            self.depth_stencil_texture = cfg.depth_stencil_config.as_ref().map(|ds_cfg| {
                let mut desc = texture_descriptor(self.size.0, self.size.1);
                desc.format = ds_cfg.format;
                desc.usage |= ds_cfg.usages;
                with_view(device.create_texture(&desc))
            })
        }
        self.resized = false;
        Ok(())
    }

    /// called at the end of rendering, this will drop the [SurfaceTexture]
    pub fn present(&mut self) {
        if let Some((t, _)) = self.color_texture.take() {
            t.present();
        }
    }

    pub(crate) fn set_size(&mut self, size: (u32, u32)) {
        if size != self.size {
            self.size = size;
            self.resized = true;
        }
    }

    fn changes(&self) -> (bool, bool, bool) {
        if self.current_config.is_none() {
            return (true, true, true);
        }
        if self.scheduled_config.is_none() {
            return (false, false, false);
        }
        let cur = self.current_config.as_ref().unwrap();
        let new = self.scheduled_config.as_ref().unwrap();
        (
            cur.color_config != new.color_config
                || cur.present_mode != new.present_mode
                || cur.backup_present_mode != new.backup_present_mode,
            cur.color_config.multisample_config != new.color_config.multisample_config,
            cur.depth_stencil_config != new.depth_stencil_config,
        )
    }

    fn scheduled_config_mut(&mut self) -> &mut SurfaceRenderTargetConfig {
        self.scheduled_config.get_or_insert_with(|| {
            self.current_config
                .as_ref()
                .expect("no scheduled or current config")
                .clone()
        })
    }

    fn current_or_scheduled_config(&self) -> &SurfaceRenderTargetConfig {
        self.current_config
            .as_ref()
            .or(self.scheduled_config.as_ref())
            .expect("no scheduled or current config")
    }
}

// Crimes against DRY
// Sorry
impl RenderTargetImpl for SurfaceRenderTarget {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn texture(&self) -> Option<&Texture> {
        self.color_texture.as_ref().map(|(c, _)| &c.texture)
    }

    fn texture_view(&self) -> Option<&TextureView> {
        self.color_texture.as_ref().map(|(_, v)| v)
    }

    fn multisampled_view(&self) -> Option<&TextureView> {
        self.multisampled_texture.as_ref().map(|(_, v)| v)
    }

    fn depth_stencil(&self) -> Option<&Texture> {
        self.depth_stencil_texture.as_ref().map(|(t, _)| t)
    }

    fn depth_stencil_view(&self) -> Option<&TextureView> {
        self.depth_stencil_texture.as_ref().map(|(_, v)| v)
    }

    fn current_color_config(&self) -> Option<&RenderTargetColorConfig> {
        self.current_config.as_ref().map(|c| &c.color_config)
    }

    fn scheduled_color_config(&self) -> Option<&RenderTargetColorConfig> {
        self.scheduled_config.as_ref().map(|c| &c.color_config)
    }

    fn scheduled_color_config_mut(&mut self) -> Option<&mut RenderTargetColorConfig> {
        Some(&mut self.scheduled_config_mut().color_config)
    }

    fn set_scheduled_color_config(&mut self, config: RenderTargetColorConfig) {
        self.scheduled_config_mut().color_config = config;
    }

    fn current_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig> {
        self.current_config
            .as_ref()
            .map(|c| c.depth_stencil_config.as_ref())?
    }

    fn scheduled_depth_stencil_config(&self) -> Option<&RenderTargetDepthStencilConfig> {
        self.scheduled_config
            .as_ref()
            .map(|c| c.depth_stencil_config.as_ref())?
    }

    fn scheduled_depth_stencil_config_mut(
        &mut self,
    ) -> Option<&mut RenderTargetDepthStencilConfig> {
        self.scheduled_config_mut().depth_stencil_config.as_mut()
    }

    fn set_scheduled_depth_stencil_config(&mut self, config: RenderTargetDepthStencilConfig) {
        self.scheduled_config_mut().depth_stencil_config = Some(config);
    }

    fn schedule_clear_color(&mut self) {
        self.clear_color_scheduled = true;
    }

    fn schedule_clear_depth(&mut self) {
        self.clear_depth_scheduled = true;
    }

    fn schedule_clear_stencil(&mut self) {
        self.clear_stencil_scheduled = true;
    }

    fn schedule_resolve(&mut self) {
        self.resolve_scheduled = true;
    }

    fn scheduled_resolve(&self) -> bool {
        self.resolve_scheduled
    }

    fn clearing(&self) -> (bool, bool, bool) {
        (
            self.clear_color_scheduled,
            self.clear_depth_scheduled,
            self.clear_stencil_scheduled,
        )
    }

    fn pass_created(&mut self) {
        self.clear_color_scheduled = false;
        self.clear_depth_scheduled = false;
        self.clear_stencil_scheduled = false;
    }

    fn unschedule_resolve(&mut self) {
        self.resolve_scheduled = false;
    }
}

fn texture_descriptor(width: u32, height: u32) -> TextureDescriptor<'static> {
    TextureDescriptor {
        label: None,
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    }
}

fn with_view(t: Texture) -> (Texture, TextureView) {
    // FIXME customization?
    let v = t.create_view(&TextureViewDescriptor::default());
    (t, v)
}
