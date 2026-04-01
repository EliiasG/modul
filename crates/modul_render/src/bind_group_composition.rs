use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use modul_core::{DeviceRes, Init};
use std::borrow::Cow;
use std::marker::PhantomData;
use std::num::NonZero;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferDescriptor, BufferUsages,
    Device, PipelineLayout, PipelineLayoutDescriptor, Queue, ShaderModule, ShaderModuleDescriptor,
    ShaderRuntimeChecks, ShaderSource, ShaderStages,
};

/// Provides a created bind group layout and its WGSL library source.
/// Use `#BIND_GROUP` as the bind group index placeholder in the library.
pub trait BindGroupLayoutProvider {
    /// The created bind group layout.
    fn layout(&self) -> &wgpu::BindGroupLayout;

    /// WGSL source declaring the bind group's bindings.
    /// Use `#BIND_GROUP` as the group index placeholder.
    fn library(&self) -> &str;
}

/// Static definition of a bind group layout. Use with [`CachedBindGroupLayout`] and
/// [`BindGroupLayoutInitPlugin`] to create and cache the layout as a resource.
pub trait BindGroupLayoutDef {
    const LAYOUT: &'static BindGroupLayoutDescriptor<'static>;
    const LIBRARY: &'static str;
}

/// Cached bind group layout created from a [`BindGroupLayoutDef`].
/// Insert as a resource via [`BindGroupLayoutInitPlugin`].
#[derive(Resource)]
pub struct CachedBindGroupLayout<P: BindGroupLayoutDef> {
    layout: wgpu::BindGroupLayout,
    _marker: PhantomData<P>,
}

impl<P: BindGroupLayoutDef> CachedBindGroupLayout<P> {
    pub fn new(device: &Device) -> Self {
        Self {
            layout: device.create_bind_group_layout(P::LAYOUT),
            _marker: PhantomData,
        }
    }
}

impl<P: BindGroupLayoutDef + Send + Sync + 'static> BindGroupLayoutProvider
    for CachedBindGroupLayout<P>
{
    fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    fn library(&self) -> &str {
        P::LIBRARY
    }
}

/// Plugin that creates a [`CachedBindGroupLayout<P>`] resource during [`Init`].
pub struct BindGroupLayoutInitPlugin<P: BindGroupLayoutDef> {
    _marker: PhantomData<P>,
}

impl<P: BindGroupLayoutDef> BindGroupLayoutInitPlugin<P> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<P: BindGroupLayoutDef + Send + Sync + 'static> Plugin for BindGroupLayoutInitPlugin<P> {
    fn build(&self, app: &mut App) {
        app.add_systems(Init, init_bind_group_layout::<P>);
    }
}

fn init_bind_group_layout<P: BindGroupLayoutDef + Send + Sync + 'static>(
    mut commands: Commands,
    device: Res<DeviceRes>,
) {
    commands.insert_resource(CachedBindGroupLayout::<P>::new(&device.0));
}

// --- BindGroupProvider ---

pub trait BindGroupProvider {
    fn bind_group(&self) -> &BindGroup;
}

pub struct SimpleBindGroupProvider {
    bind_group: BindGroup,
    uniform_buffers: Vec<Buffer>,
}

impl SimpleBindGroupProvider {
    pub fn set_uniform<Ty: UniformType>(
        &self,
        queue: &Queue,
        entry: &UniformEntry<Ty>,
        value: Ty::Resource,
    ) {
        let buffer = &self.uniform_buffers[entry.0 as usize];
        let mut bytes = vec![0u8; entry.1];
        Ty::set_bytes(value, &mut bytes);
        queue.write_buffer(buffer, 0, &bytes);
    }
}

impl BindGroupProvider for SimpleBindGroupProvider {
    fn bind_group(&self) -> &BindGroup {
        &self.bind_group
    }
}

// --- SimpleBindGroupLayoutProvider ---

pub struct SimpleBindGroupLayoutProvider {
    /// (binding index, byte size) for each uniform
    uniform_info: Vec<(u32, usize)>,
    library: String,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl SimpleBindGroupLayoutProvider {
    pub fn build_bind_group(
        &self,
        device: &Device,
        binding_entries: &[(BindingEntry, BindingResource)],
    ) -> SimpleBindGroupProvider {
        let uniform_buffers: Vec<Buffer> = self
            .uniform_info
            .iter()
            .map(|(_, size)| {
                device.create_buffer(&BufferDescriptor {
                    label: None,
                    size: *size as u64,
                    usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();

        let mut bg_entries: Vec<BindGroupEntry> = binding_entries
            .iter()
            .map(|(entry, resource)| BindGroupEntry {
                binding: entry.0,
                resource: resource.clone(),
            })
            .collect();

        for (i, (binding_idx, _)) in self.uniform_info.iter().enumerate() {
            bg_entries.push(BindGroupEntry {
                binding: *binding_idx,
                resource: uniform_buffers[i].as_entire_binding(),
            });
        }

        SimpleBindGroupProvider {
            bind_group: device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &self.bind_group_layout,
                entries: &bg_entries,
            }),
            uniform_buffers,
        }
    }
}

impl BindGroupLayoutProvider for SimpleBindGroupLayoutProvider {
    fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    fn library(&self) -> &str {
        &self.library
    }
}

// --- SimpleBindGroupLayoutBuilder ---

pub struct SimpleBindGroupLayoutBuilder {
    entries: Vec<EntryData>,
    uniform_count: u32,
}

impl SimpleBindGroupLayoutBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            uniform_count: 0,
        }
    }

    pub fn add_entry(
        &mut self,
        name: String,
        wgsl_type_name: String,
        visibility: ShaderStages,
        ty: BindingType,
        count: Option<NonZero<u32>>,
    ) -> BindingEntry {
        let binding = self.entries.len() as u32;
        self.entries.push(EntryData::Binding(
            (name, wgsl_type_name),
            BindGroupLayoutEntry {
                binding,
                visibility,
                ty,
                count,
            },
        ));
        BindingEntry(binding)
    }

    pub fn add_uniform<Ty: UniformType>(&mut self, name: String) -> UniformEntry<Ty> {
        let uniform_type = Ty::wgsl_uniform_type();
        let uniform_index = self.uniform_count;
        self.uniform_count += 1;
        self.entries
            .push(EntryData::Uniform(name, uniform_type));
        UniformEntry(uniform_index, uniform_type.byte_size(), PhantomData)
    }

    pub fn build(self, device: &Device) -> SimpleBindGroupLayoutProvider {
        let mut layout_entries = Vec::new();
        let mut uniform_info = Vec::new();
        let mut library_lines = Vec::new();

        for (i, entry) in self.entries.iter().enumerate() {
            let binding = i as u32;
            match entry {
                EntryData::Binding((name, tname), layout_entry) => {
                    layout_entries.push(*layout_entry);
                    library_lines.push(format!(
                        "@group(#BIND_GROUP) @binding({binding})\nvar {name}: {tname};"
                    ));
                }
                EntryData::Uniform(name, uniform_type) => {
                    layout_entries.push(BindGroupLayoutEntry {
                        binding,
                        visibility: ShaderStages::VERTEX_FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZero::new(uniform_type.byte_size() as u64),
                        },
                        count: None,
                    });
                    uniform_info.push((binding, uniform_type.byte_size()));
                    let tname = uniform_type.wgsl_type_name();
                    library_lines.push(format!(
                        "@group(#BIND_GROUP) @binding({binding})\nvar<uniform> {name}: {tname};"
                    ));
                }
            }
        }

        let library = library_lines.join("\n");

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Simple BGLayout"),
            entries: &layout_entries,
        });

        SimpleBindGroupLayoutProvider {
            uniform_info,
            library,
            bind_group_layout,
        }
    }
}

pub struct BindingEntry(u32);

/// Typed handle for a uniform entry. Used with [`SimpleBindGroupProvider::set_uniform`].
/// Stores the uniform buffer index and byte size.
pub struct UniformEntry<Ty: UniformType>(u32, usize, PhantomData<Ty>);

pub trait UniformType {
    type Resource;

    fn set_bytes(res: Self::Resource, bytes: &mut [u8]);

    fn wgsl_uniform_type() -> WgslUniformType;
}

#[derive(Clone, Copy)]
pub enum WgslUniformType {
    Float,
    Vec2,
    Vec3,
    Vec4,
    Mat3x3,
    Mat4x4,
}

impl WgslUniformType {
    pub fn wgsl_type_name(self) -> &'static str {
        match self {
            Self::Float => "f32",
            Self::Vec2 => "vec2<f32>",
            Self::Vec3 => "vec3<f32>",
            Self::Vec4 => "vec4<f32>",
            Self::Mat3x3 => "mat3x3<f32>",
            Self::Mat4x4 => "mat4x4<f32>",
        }
    }

    pub fn byte_size(self) -> usize {
        match self {
            Self::Float => 4,
            Self::Vec2 => 8,
            Self::Vec3 => 12,
            Self::Vec4 => 16,
            Self::Mat3x3 => 48,
            Self::Mat4x4 => 64,
        }
    }
}

enum EntryData {
    Binding((String, String), BindGroupLayoutEntry),
    Uniform(String, WgslUniformType),
}

// --- PipelineLayoutComposer ---

/// Composes a pipeline layout and shader from multiple [`BindGroupLayoutProvider`]s.
/// Bind group WGSL libraries are concatenated with `#BIND_GROUP` replaced by the group index.
/// Additional WGSL snippets can be prepended before the main shader source.
pub struct PipelineLayoutComposer {
    source: Vec<Box<dyn BindGroupLayoutProvider + Send + Sync>>,
    snippets: Vec<String>,
    composed: Option<PipelineLayout>,
    compiled_shader: Option<ShaderModule>,
    checks: Option<ShaderRuntimeChecks>,
}

impl PipelineLayoutComposer {
    pub fn new() -> Self {
        Self {
            source: Vec::new(),
            snippets: Vec::new(),
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

    /// Add a bind group layout provider. The provider's WGSL library will be included
    /// in composed shaders with `#BIND_GROUP` replaced by the group index.
    #[inline]
    pub fn add_bind_group(
        &mut self,
        group: impl BindGroupLayoutProvider + Send + Sync + 'static,
    ) -> &mut Self {
        self.source.push(Box::new(group));
        self.composed = None;
        self.compiled_shader = None;
        self
    }

    /// Add a WGSL snippet that will be included before the main shader source.
    /// Use this for shared utility functions (e.g. lighting, depth reconstruction).
    #[inline]
    pub fn add_snippet(&mut self, source: impl Into<String>) -> &mut Self {
        self.snippets.push(source.into());
        self.compiled_shader = None;
        self
    }

    /// Compose and cache the pipeline layout from the added bind group providers.
    pub fn compose_pipeline_layout(&mut self, device: &Device) -> &PipelineLayout {
        self.composed.get_or_insert_with(|| {
            let layouts: Vec<&wgpu::BindGroupLayout> =
                self.source.iter().map(|p| p.layout()).collect();
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("Composed pipeline layout"),
                bind_group_layouts: &layouts,
                push_constant_ranges: &[],
            })
        })
    }

    /// Gets the currently cached pipeline layout.
    #[inline]
    pub fn get_pipeline_layout(&self) -> Option<&PipelineLayout> {
        self.composed.as_ref()
    }

    /// Compose and cache a shader module from bind group libraries, snippets, and the main source.
    pub fn compose_shader(
        &mut self,
        device: &Device,
        label: &str,
        main_source: &str,
    ) -> &ShaderModule {
        if self.compiled_shader.is_some() {
            return self.compiled_shader.as_ref().unwrap();
        }

        let mut full_source = String::new();

        // Bind group declarations
        for (i, group) in self.source.iter().enumerate() {
            let lib = group.library().replace("#BIND_GROUP", &i.to_string());
            full_source.push_str(&lib);
            full_source.push('\n');
        }

        // Shared snippets
        for snippet in &self.snippets {
            full_source.push_str(snippet);
            full_source.push('\n');
        }

        // Main shader
        full_source.push_str(main_source);

        let desc = ShaderModuleDescriptor {
            label: Some(label),
            source: ShaderSource::Wgsl(Cow::Owned(full_source)),
        };

        self.compiled_shader = Some(match self.checks {
            None => device.create_shader_module(desc),
            Some(checks) => unsafe { device.create_shader_module_trusted(desc, checks) },
        });

        self.compiled_shader.as_ref().unwrap()
    }

    /// Gets the currently cached shader module.
    #[inline]
    pub fn get_shader_module(&self) -> Option<&ShaderModule> {
        self.compiled_shader.as_ref()
    }
}
