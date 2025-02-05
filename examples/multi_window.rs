use bevy_ecs::prelude::*;
use modul_asset::Assets;
use modul_core::{run_app, DefaultGraphicsInitializer, Init, InitialWindowConfig, MainWindow, UpdatingWindow};
use modul_render::{
    ClearNext, EmptyPass, PreDraw, RenderPlugin, RenderTarget, RenderTargetSource,
    RunningSequenceQueue, Sequence, SequenceBuilder, SequenceQueue, SurfaceRenderTarget,
};
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

#[derive(Component)]
struct MyWindow {
    offset: u64,
}

fn init_sequence(
    surface_query: Query<Entity, With<MainWindow>>,
    mut sequence_assets: ResMut<Assets<Sequence>>,
    mut commands: Commands,
) {
    let win_id = surface_query.single();
    commands.entity(win_id).insert(MyWindow { offset: 0 });
    
    let render_target = RenderTargetSource::Surface(win_id);
    let mut builder = SequenceBuilder::new();
    builder
        .add(ClearNext { render_target })
        .add(EmptyPass { render_target });
    for i in 1..=2 {
        let id = commands
            .spawn((
                InitialWindowConfig {
                    window_attribs: WindowAttributes::default().with_title(format!("window {}", i)),
                },
                MyWindow { offset: i * 60 },
                UpdatingWindow,
            ))
            .id();
        let render_target = RenderTargetSource::Surface(id);
        builder.add(ClearNext { render_target });
        builder.add(EmptyPass { render_target });
    }
    commands.insert_resource(FrameCount(0));
    commands.insert_resource(RunningSequenceQueue(SequenceQueue(vec![
        builder.finish(&mut sequence_assets)
    ])));
}

fn set_color(
    mut frame_count: ResMut<FrameCount>,
    mut rt_query: Query<(&mut SurfaceRenderTarget, &MyWindow)>,
) {
    frame_count.0 += 1;

    for (mut rt, window) in rt_query.iter_mut() {
        let n = frame_count.0 + window.offset;
        rt.set_clear_color(Color {
            r: (n % 200) as f64 / 200.0,
            g: (n % 600) as f64 / 600.0,
            b: (n % 1800) as f64 / 1800.0,
            a: 1.0,
        });
    }
}
