use bevy_ecs::prelude::*;
use modul_asset::Assets;
use modul_core::{run_app, DefaultGraphicsInitializer, Init, MainWindow};
use modul_render::{ClearNext, EmptyPass, PreDraw, RenderPlugin, RenderTarget, RenderTargetSource, RunningSequenceQueue, Sequence, SequenceBuilder, SequenceQueue, SurfaceRenderTarget};
use modul_util::ExitPlugin;
use wgpu::{Color, PowerPreference};
use winit::window::WindowAttributes;

fn main() {
    run_app(
        DefaultGraphicsInitializer {
            power_preference: PowerPreference::None,
            window_attribs: WindowAttributes::default(),
        },
        |app| {
            app.add_plugins((RenderPlugin, ExitPlugin));
            app.add_systems(PreDraw, set_color);
            app.add_systems(Init, init_sequence);
        },
    );
}

#[derive(Resource)]
struct FrameCount(u64);

fn init_sequence(
    surface_query: Query<Entity, With<MainWindow>>,
    mut sequence_assets: ResMut<Assets<Sequence>>,
    mut commands: Commands,
) {
    let render_target = RenderTargetSource::Surface(surface_query.single());
    let mut builder = SequenceBuilder::new();
    builder
        .add(ClearNext { render_target })
        .add(EmptyPass { render_target });
    commands.insert_resource(FrameCount(0));
    commands.insert_resource(RunningSequenceQueue(SequenceQueue(vec![builder.finish(&mut sequence_assets)])));
}

fn set_color(
    mut frame_count: ResMut<FrameCount>,
    mut surface_query: Query<&mut SurfaceRenderTarget, With<MainWindow>>,
) {
    frame_count.0 += 1;
    
    let Ok(mut rt) = surface_query.get_single_mut() else {
        return;
    };
    rt.set_clear_color(Color {
        r: (frame_count.0 % 200) as f64 / 200.0,
        g: (frame_count.0 % 600) as f64 / 600.0,
        b: (frame_count.0 % 1800) as f64 / 1800.0,
        a: 1.0,
    });
}
