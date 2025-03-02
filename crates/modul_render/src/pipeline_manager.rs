use crate::{DynBindGroupLayoutProvider, PipelineLayoutProvider, RenderTarget, RenderTargetSource};
use bevy_ecs::world::World;
use modul_asset::{AssetId, Assets};
use modul_core::DeviceRes;
use modul_util::HashMap;
use std::ops::Deref;
use naga_oil::compose::NagaModuleDescriptor;
use wgpu::{BlendState, BufferAddress, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState, Device, FragmentState, MultisampleState, PipelineLayout, PrimitiveState, RenderPipeline, RenderPipelineDescriptor, ShaderModule, StencilState, TextureFormat, VertexAttribute, VertexBufferLayout, VertexState, VertexStepMode};

/// A stripped version of [RenderPipelineDescriptor] that removes multisample and format information.
/// This is useful to define pipelines without knowing anything about the textures.
pub struct GenericRenderPipelineDescriptor {
    pub label: Option<String>,
    pub layout: AssetId<PipelineLayoutProvider>,
    pub vertex_state: GenericVertexState,
    pub primitive: PrimitiveState,
    pub depth_stencil: Option<GenericDepthStencilState>,
    pub multisample: GenericMultisampleState,
    pub fragment: Option<GenericFragmentState>,
}

/// Used with [GenericRenderPipelineDescriptor]
pub struct GenericVertexState {
    pub module: AssetId<>,
    pub entry_point: String,
    pub buffers: Vec<GenericVertexBufferLayout>,
}

/// Used with [GenericVertexState]
pub struct GenericVertexBufferLayout {
    pub array_stride: BufferAddress,
    pub step_mode: VertexStepMode,
    pub attributes: Vec<VertexAttribute>,
}

/// Used with [GenericRenderPipelineDescriptor]
pub struct GenericDepthStencilState {
    pub depth_write_enable: bool,
    pub depth_compare: CompareFunction,
    pub stencil: StencilState,
    pub bias: DepthBiasState,
}

/// Used with [GenericRenderPipelineDescriptor]
pub struct GenericMultisampleState {
    pub mask: u64,
    pub alpha_to_coverage_enabled: bool,
}

/// Used with [GenericRenderPipelineDescriptor]
pub struct GenericFragmentState {
    // TODO naga_oil
    pub module: AssetId<ShaderModule>,
    pub entry_point: String,
    /// Blend state of the possible target
    pub target_blend: Option<BlendState>,
    /// Color write of the possible target
    pub target_color_writes: ColorWrites,
}

/// Used with [GenericPipelineDescriptor] to describe a pipeline.  
/// The parameters are invalid if both color_format and depth_stencil_format are None.  
#[derive(Hash, Clone, PartialEq, Eq, Debug)]
pub struct PipelineParameters {
    pub color_format: Option<TextureFormat>,
    pub depth_stencil_format: Option<TextureFormat>,
    pub sample_count: u32,
}

/// Used to manage instances of a [GenericRenderPipelineDescriptor]
pub struct RenderPipelineManager {
    desc: GenericRenderPipelineDescriptor,
    instances: HashMap<PipelineParameters, RenderPipeline>,
}

impl RenderPipelineManager {
    pub fn new(desc: GenericRenderPipelineDescriptor) -> Self {
        if desc.fragment.is_none() && desc.depth_stencil.is_none() {
            panic!("must have either fragment or depth_stencil");
        }
        Self {
            desc,
            instances: HashMap::new(),
        }
    }

    /// Returns if the pipeline can be created for a parameter with only depth_stencil.  
    pub fn has_depth_stencil(&self) -> bool {
        self.desc.depth_stencil.is_some()
    }

    /// Returns if the pipeline can be created for a parameter with only color.  
    pub fn has_color(&self) -> bool {
        self.desc.fragment.is_some()
    }

    /// Gets a pipeline from the internal cache, or creates and stores one given the parameters.  
    /// The returned value can be ignored if you just want to init the pipeline.  
    pub fn get(
        &mut self,
        params: &PipelineParameters,
        device: &Device,
        layouts: &Assets<PipelineLayout>,
        shaders: &Assets<ShaderModule>,
    ) -> &RenderPipeline {
        if params.color_format.is_none() && params.depth_stencil_format.is_none() {
            panic!("color_format and depth_stencil_format must not both be none");
        }
        if !self.has_depth_stencil() && params.color_format.is_none() {
            panic!("no color_format on pipeline that only supports color");
        }
        if !self.has_color() && params.depth_stencil_format.is_none() {
            panic!("no depth_stencil format on pipeline that only supports depth_stencil");
        }

        self.instances.entry(params.clone()).or_insert_with(|| {
            let module = shaders
                .get(self.desc.vertex_state.module)
                .expect("shader does not exist");

            let mut targets = Vec::new();

            let desc = RenderPipelineDescriptor {
                label: self.desc.label.as_ref().map(String::as_str),
                layout: Some(
                    layouts
                        .get(self.desc.layout)
                        .expect("layout does not exist"),
                ),
                vertex: VertexState {
                    module,
                    entry_point: Some(self.desc.vertex_state.entry_point.as_str()),
                    compilation_options: Default::default(),
                    buffers: &self
                        .desc
                        .vertex_state
                        .buffers
                        .iter()
                        .map(|l| VertexBufferLayout {
                            array_stride: l.array_stride,
                            step_mode: l.step_mode,
                            attributes: &l.attributes,
                        })
                        .collect::<Box<_>>(),
                },
                primitive: self.desc.primitive.clone(),
                depth_stencil: params.depth_stencil_format.and_then(|format| {
                    self.desc
                        .depth_stencil
                        .as_ref()
                        .map(|ds| DepthStencilState {
                            format,
                            depth_write_enabled: ds.depth_write_enable,
                            depth_compare: ds.depth_compare,
                            stencil: ds.stencil.clone(),
                            bias: ds.bias,
                        })
                }),
                multisample: MultisampleState {
                    count: params.sample_count,
                    mask: self.desc.multisample.mask,
                    alpha_to_coverage_enabled: self.desc.multisample.alpha_to_coverage_enabled,
                },
                /*
                fragment: params.color_format.and_then(|format| {
                    self.desc.fragment.as_ref().map(|frag|
                }),*/
                fragment: if let (Some(format), Some(frag)) = (params.color_format, self.desc.fragment.as_ref()) {
                    targets.push(Some(ColorTargetState{
                        format,
                        blend: frag.target_blend,
                        write_mask: frag.target_color_writes,
                    }));
                    Some(FragmentState {
                        module,
                        entry_point: Some(frag.entry_point.as_str()),
                        compilation_options: Default::default(),
                        targets: &targets,
                    })
                } else {None},
                multiview: None,
                cache: None,
            };
            device.create_render_pipeline(&desc)
        })
    }

    /// Gets the pipeline for a [RenderTarget], see [Self::get] for more details.  
    /// This can also be used for initialization
    pub fn get_compatible(
        &mut self,
        render_target: RenderTargetSource,
        world: &World,
    ) -> Option<&RenderPipeline> {
        let render_target = render_target.get(world)?;
        let device = &world.resource::<DeviceRes>().0;
        let layouts = world.resource();
        let shaders = world.resource();
        let color_format = render_target.texture().map(|t| t.format());
        let depth_stencil_format = render_target.depth_stencil().map(|t| t.format());
        if color_format.is_none() && depth_stencil_format.is_none() {
            return None;
        }
        if !self.has_color() && depth_stencil_format.is_none() {
            return None;
        }
        if !self.has_depth_stencil() && color_format.is_none() {
            return None;
        }
        Some(self.get(
            &PipelineParameters {
                color_format,
                depth_stencil_format,
                sample_count: render_target.sample_count(),
            },
            device,
            layouts,
            shaders,
        ))
    }
}
