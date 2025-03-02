use naga_oil::compose::{ComposableModuleDescriptor, Composer, ComposerError, NagaModuleDescriptor, ShaderLanguage};
use wgpu::{BindGroupLayoutDescriptor, Device, PipelineLayout, PipelineLayoutDescriptor};
use wgpu::naga::Module;

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

pub struct PipelineLayoutProvider {
    source: Vec<Box<dyn BindGroupLayoutProvider + Send + Sync>>,
    composed: Option<PipelineLayout>,
}

impl PipelineLayoutProvider {
    fn new() -> Self {
        Self {
            source: Vec::new(),
            composed: None,
        }
    }

    #[inline]
    fn add_bind_group_layout(
        &mut self,
        group: impl BindGroupLayoutProvider + Send + Sync + 'static,
    ) -> &mut Self {
        self.source.push(Box::new(group));
        self.composed = None;
        self
    }

    fn get_composed(&mut self, device: &Device) -> &PipelineLayout {
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

    /// Will compose a naga module, adding shader libraries from bind groups
    fn compose_naga(&self, composer: &mut Composer, desc: NagaModuleDescriptor) -> Result<Module, ComposerError> {
        let mut deps = Vec::new();
        for (i, group) in self.source.iter().enumerate() {
            let str_i = i.to_string();
            let name = format!("GEN_bindgroup_X_{i}");
            deps.push(name.clone());
            let mod_source = group
                .library()
                .to_string()
                .replace("#BIND_GROUP", &str_i);
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
