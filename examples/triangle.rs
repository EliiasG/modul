/// Triangle rendering using the low level functions of modul.
use bevy_ecs::prelude::*;
use modul::asset::{AssetId, AssetWorldExt, Assets};
use modul::core::{run_app, DefaultGraphicsInitializer, DeviceRes, Init, MainWindow};
use modul::render::{
    ClearNext, GenericFragmentState, GenericMultisampleState, GenericRenderPipelineDescriptor,
    GenericVertexState, InitialSurfaceConfig, Operation, OperationBuilder, RenderPipelineManager,
    RenderPlugin, RenderTargetColorConfig, RenderTargetMultisampleConfig, RenderTargetSource,
    RunningSequenceQueue, Sequence, SequenceBuilder, SequenceQueue, SurfaceRenderTargetConfig,
};
use modul::util::ExitPlugin;
use modul_render::DirectRenderPipelineResourceProvider;
use wgpu::{
    BlendState, Color, ColorWrites, CommandEncoder, Device, FrontFace, PipelineLayout,
    PipelineLayoutDescriptor, PolygonMode, PowerPreference, PresentMode, PrimitiveState,
    PrimitiveTopology, ShaderModule, ShaderModuleDescriptor, ShaderSource, TextureUsages,
};
use winit::window::WindowAttributes;


fn main() {
    run_app(
        DefaultGraphicsInitializer {
            power_preference: PowerPreference::None,
            window_attribs: WindowAttributes::default(),
        },
        |app| {
            app.add_plugins((RenderPlugin, ExitPlugin));
            app.add_systems(Init, (init_color, init_pipeline, init_sequence).chain());
        },
    );
}

#[derive(Resource)]
struct TrianglePipeline(AssetId<RenderPipelineManager>);

fn init_color(mut commands: Commands, query: Query<Entity, With<MainWindow>>) {
    commands
        .entity(query.single())
        .insert(InitialSurfaceConfig(SurfaceRenderTargetConfig {
            color_config: RenderTargetColorConfig {
                multisample_config: Some(RenderTargetMultisampleConfig { sample_count: 4 }),
                clear_color: Color {
                    r: 0.25,
                    g: 0.5,
                    b: 0.5,
                    a: 1.0,
                },
                usages: TextureUsages::RENDER_ATTACHMENT,
                format_override: None,
            },
            depth_stencil_config: None,
            desired_maximum_frame_latency: 2,
            present_mode: PresentMode::AutoVsync,
            backup_present_mode: None,
        }));
}

fn init_pipeline(
    mut commands: Commands,
    device: Res<DeviceRes>,
    mut shaders: ResMut<Assets<ShaderModule>>,
    mut layouts: ResMut<Assets<PipelineLayout>>,
    mut piplines: ResMut<Assets<RenderPipelineManager>>,
) {
    let shader = shaders.add(device.0.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(include_str!("assets/triangle.wgsl").into()),
    }));
    let layout = layouts.add(device.0.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    }));
    let desc = GenericRenderPipelineDescriptor {
        resource_provider: Box::new(DirectRenderPipelineResourceProvider {
            layout,
            vertex_shader_module: shader,
            fragment_shader_module: shader,
        }),
        label: None,
        vertex_state: GenericVertexState {
            entry_point: "vs_main".to_string(),
            buffers: vec![],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: GenericMultisampleState {
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(GenericFragmentState {
            entry_point: "fs_main".to_string(),
            target_blend: Some(BlendState::REPLACE),
            target_color_writes: ColorWrites::ALL,
        }),
    };
    let id = piplines.add(RenderPipelineManager::new(desc));
    commands.insert_resource(TrianglePipeline(id));
}

fn init_sequence(
    surface_query: Query<Entity, With<MainWindow>>,
    mut sequence_assets: ResMut<Assets<Sequence>>,
    mut commands: Commands,
) {
    let render_target = RenderTargetSource::Surface(surface_query.single());
    let mut builder = SequenceBuilder::new();
    builder
        .add(ClearNext { render_target })
        .add(TriangleOperationBuilder(render_target));
    commands.insert_resource(RunningSequenceQueue(SequenceQueue(vec![
        builder.finish(&mut sequence_assets)
    ])));
}

struct TriangleOperation {
    target: RenderTargetSource,
}

impl Operation for TriangleOperation {
    fn run(&mut self, world: &mut World, command_encoder: &mut CommandEncoder) {
        let id = world.resource::<TrianglePipeline>().0;
        world.asset_scope(id, |world, pipeline_man| {
            let Some(pipeline) = pipeline_man.get_compatible(self.target, world) else {
                return;
            };
            let Some(mut rt) = self.target.get_mut(world) else {
                return;
            };
            let Some(mut pass) = rt.begin_ending_pass(command_encoder) else {
                return;
            };
            pass.set_pipeline(pipeline);
            pass.draw(0..3, 0..1);
        });
    }
}

struct TriangleOperationBuilder(RenderTargetSource);

impl OperationBuilder for TriangleOperationBuilder {
    fn reading(&self) -> Vec<RenderTargetSource> {
        Vec::new()
    }

    fn writing(&self) -> Vec<RenderTargetSource> {
        vec![self.0]
    }

    fn finish(self, _world: &World, _device: &Device) -> impl Operation + 'static {
        TriangleOperation { target: self.0 }
    }
}
