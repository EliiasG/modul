use crate::RenderPipelineResourceProvider;
use bevy_ecs::world::{Mut, World};
use modul_asset::{AssetId, AssetWorldExt};
use modul_core::DeviceRes;
use naga_oil::compose::{
    ComposableModuleDescriptor, Composer, ComposerError, ImportDefinition, NagaModuleDescriptor,
    ShaderDefValue, ShaderLanguage, ShaderType,
};
use std::borrow::Cow;
use wgpu::naga::Module;
use wgpu::{
    BindGroupLayoutDescriptor, Device, PipelineLayout, PipelineLayoutDescriptor, ShaderModule,
    ShaderModuleDescriptor, ShaderRuntimeChecks, ShaderSource,
};

/// Provides a bind group shaders, for static cases use [ConstBindGroupLayoutProvider]
pub trait BindGroupLayoutProvider {
    /// The provided layout. May be generated at runtime.  
    fn layout(&self) -> &BindGroupLayoutDescriptor;

    /// The naga_oil WGSL source of a library implementing the bind group shader-side.  
    /// Use #BIND_GROUP as the bind group index
    fn library(&self) -> &str;
}

/// [BindGroupLayoutProvider] is automatically implemented for implementations of this.  
pub trait ConstBindGroupLayoutProvider {
    const LAYOUT: &'static BindGroupLayoutDescriptor<'static>;

    const LIBRARY: &'static str;
}

impl<T: ConstBindGroupLayoutProvider> BindGroupLayoutProvider for T {
    fn layout(&self) -> &BindGroupLayoutDescriptor {
        T::LAYOUT
    }

    fn library(&self) -> &str {
        T::LIBRARY
    }
}

/// Generates and caches a [PipelineLayout] using [BindGroupProviders](BindGroupProvider).  
/// Also generates shaders, since they need information about bind groups.  
/// Can be used as a [RenderPipelineResourceProvider] using a [ComposedPipelineResourceProvider].  
pub struct PipelineLayoutGenerator {
    source: Vec<Box<dyn BindGroupLayoutProvider + Send + Sync>>,
    composed: Option<PipelineLayout>,
    compiled_shader: Option<ShaderModule>,
    checks: Option<ShaderRuntimeChecks>,
}

impl PipelineLayoutGenerator {
    pub fn new() -> Self {
        Self {
            source: Vec::new(),
            composed: None,
            compiled_shader: None,
            checks: None,
        }
    }

    /// # Safety
    /// See [create_shader_module_trusted](Device::create_shader_module_trusted)
    pub unsafe fn new_trusted(checks: ShaderRuntimeChecks) -> Self {
        Self {
            checks: Some(checks),
            ..Self::new()
        }
    }

    #[inline]
    pub fn add_bind_group_layout(
        &mut self,
        group: impl BindGroupLayoutProvider + Send + Sync + 'static,
    ) -> &mut Self {
        self.source.push(Box::new(group));
        self.composed = None;
        self.compiled_shader = None;
        self
    }

    /// Will compose and cache the pipeline layout.
    /// Safe to call many times, as it caches and only reallocated if there are changes.  
    pub fn compose_pipeline_layout(&mut self, device: &Device) -> &PipelineLayout {
        self.composed.get_or_insert_with(|| {
            let layouts = self
                .source
                .iter()
                .map(|d| device.create_bind_group_layout(d.layout()))
                .collect::<Vec<_>>();
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("Composed pipeline layout".into()),
                // must iter twice, once to gen owned values, then reference them
                bind_group_layouts: &layouts.iter().collect::<Vec<_>>(),
                push_constant_ranges: &[],
            })
        })
    }

    /// Gets the currently cached pipeline layout.
    /// The currently cached layout will be automatically invalidated when changes are made.
    pub fn get_pipeline_layout(&self) -> Option<&PipelineLayout> {
        self.composed.as_ref()
    }

    /// Will compose a naga module, adding shader libraries from bind groups, and cache the result.
    /// Safe to call many times, as it caches and only reallocated if there are changes.  
    pub fn compose_shader(
        &mut self,
        device: &Device,
        composer: &mut Composer,
        desc: NagaModuleDescriptor,
    ) -> Result<&ShaderModule, ComposerError> {
        // cannot use if let as it borrows and causes lifetime issues...
        if self.compiled_shader.is_some() {
            return Ok(self.compiled_shader.as_ref().unwrap());
        }

        let name = format!("naga_oil shader: {}", desc.file_path);

        let res = self.generate_module(composer, desc)?;

        let desc = ShaderModuleDescriptor {
            label: Some(&name),
            source: ShaderSource::Naga(Cow::Owned(res)),
        };

        self.compiled_shader = Some(match self.checks {
            None => device.create_shader_module(desc),
            Some(checks) => unsafe { device.create_shader_module_trusted(desc, checks) },
        });

        Ok(self.compiled_shader.as_ref().unwrap())
    }

    /// Gets the currently cached shader module.
    /// The currently cached module will be automatically invalidated when changes are made.
    pub fn get_shader_module(&self) -> Option<&ShaderModule> {
        self.compiled_shader.as_ref()
    }

    fn generate_module(
        &mut self,
        composer: &mut Composer,
        desc: NagaModuleDescriptor,
    ) -> Result<Module, ComposerError> {
        let mut deps = Vec::new();
        for (i, group) in self.source.iter().enumerate() {
            let str_i = i.to_string();
            let name = format!("GEN_bindgroup_X_{i}");
            deps.push(name.clone());
            let mod_source = group.library().to_string().replace("#BIND_GROUP", &str_i);
            composer.add_composable_module(ComposableModuleDescriptor {
                source: &mod_source,
                file_path: &format!("bind_group_shader_{str_i}"),
                language: ShaderLanguage::Wgsl,
                as_name: Some(name),
                additional_imports: &[],
                shader_defs: Default::default(),
            })?;
        }
        let res = composer.make_naga_module(desc)?;
        for dep in &deps {
            composer.remove_composable_module(dep);
        }
        Ok(res)
    }
}

pub struct StaticNagaModuleDescriptor {
    pub source: String,
    pub file_path: String,
    pub shader_type: ShaderType,
    pub shader_defs: std::collections::HashMap<String, ShaderDefValue>,
    pub additional_imports: Vec<ImportDefinition>,
}

impl StaticNagaModuleDescriptor {
    pub fn get(&self) -> NagaModuleDescriptor {
        NagaModuleDescriptor {
            source: &self.source,
            file_path: &self.file_path,
            shader_type: self.shader_type,
            // unfortunately seems required
            shader_defs: self.shader_defs.clone(),
            additional_imports: &self.additional_imports,
        }
    }
}

/// Implements [RenderPipelineResourceProvider]
pub struct ComposedPipelineResourceProvider {
    pub generator: AssetId<PipelineLayoutGenerator>,
    pub desc: AssetId<StaticNagaModuleDescriptor>,
    // FIXME possibly abstract where the composer comes from
    pub composer: AssetId<Composer>,
}

impl RenderPipelineResourceProvider for ComposedPipelineResourceProvider {
    fn update(&self, world: &mut World) {
        world.resource_scope(|world, device_res: Mut<DeviceRes>| {
            world
                .asset_mut(self.generator)
                .compose_pipeline_layout(&device_res.0);
            world.asset_scope(self.generator, |world, g| {
                world.asset_scope(self.composer, |world, c| {
                    g.compose_shader(&device_res.0, c, world.asset(self.desc).get())
                        .expect("failed to compose shader");
                });
            });
        });
    }

    fn get_pipeline_layout<'a>(&self, world: &'a World) -> &'a PipelineLayout {
        world.asset(self.generator).get_pipeline_layout().unwrap()
    }

    fn get_vertex_shader_module<'a>(&self, world: &'a World) -> &'a ShaderModule {
        world.asset(self.generator).get_shader_module().unwrap()
    }

    fn get_fragment_shader_module<'a>(&self, world: &'a World) -> &'a ShaderModule {
        self.get_vertex_shader_module(world)
    }
}