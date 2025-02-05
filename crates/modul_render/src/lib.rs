mod render_target;
mod sequence;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use modul_asset::AssetAppExt;
use modul_core::{
    AdapterRes, DeviceRes, EventBuffer, ImportantWindow, Redraw, ShouldExit, SurfaceFormat,
    UpdatingWindow, WindowComponent, WindowMap,
};
use wgpu::SurfaceError;
use winit::event::{Event, WindowEvent};

pub use render_target::*;
pub use sequence::*;

/// Runs before [Synchronize] useful to pause processes that should be rendered
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct PreSynchronize;

/// Should be used to get data ready for rendering
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Synchronize;

/// Runs after [Synchronize] useful to re-start processes that have been paused for synchronization
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct PostSynchronize;

/// Runs before [Draw], and after [PostSynchronize]
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct PreDraw;

/// The main rendering schedule, this is run whenever a window requests a redraw
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Draw;

/// Runs after [Draw]
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct PostDraw;

/// [SystemSet] within [Redraw] that executes the draw schedules
#[derive(SystemSet, Clone, Hash, PartialEq, Eq, Debug)]
pub struct RenderSystemSet;

/// [SystemSet] within [Draw] that executes applies changes to all [OffscreenRenderTargets]
#[derive(SystemSet, Clone, Hash, PartialEq, Eq, Debug)]
pub struct ApplyOffscreenTargetsSystemSet;

/// [SystemSet] within [Draw] that executes the [RunningSequenceQueue]
#[derive(SystemSet, Clone, Hash, PartialEq, Eq, Debug)]
pub struct SequenceRunnerSet;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(PreSynchronize);
        app.init_schedule(Synchronize);
        app.init_schedule(PostSynchronize);
        app.init_schedule(PreDraw);
        app.init_schedule(Draw);
        app.init_schedule(PostDraw);
        app.init_assets::<Sequence>();

        app.add_systems(
            Redraw,
            (
                create_surface_targets,
                handle_events,
                draw.run_if(resource_exists::<ShouldDraw>),
            )
                .chain()
                .in_set(RenderSystemSet),
        );
        app.add_systems(
            Redraw,
            (present_surface_targets, request_redraws)
                .run_if(resource_exists::<ShouldDraw>)
                .after(RenderSystemSet),
        );
        app.add_systems(
            Draw,
            apply_offscreen_targets.in_set(ApplyOffscreenTargetsSystemSet),
        );
        app.add_systems(Draw, run_sequences.in_set(SequenceRunnerSet));
        app.configure_sets(
            Draw,
            SequenceRunnerSet.after(ApplyOffscreenTargetsSystemSet),
        );
    }
}

/// If added before [RenderSystemSet], [Synchronize] and [Draw] will run
#[derive(Resource)]
pub struct ShouldDraw;
fn handle_events(
    mut commands: Commands,
    device: Res<DeviceRes>,
    events: ResMut<EventBuffer>,
    map: Res<WindowMap>,
    mut window_query: Query<(
        &WindowComponent,
        &mut SurfaceRenderTarget,
        Has<ImportantWindow>,
    )>,
) {
    for e in events.events() {
        let Event::WindowEvent { window_id, event } = e else {
            continue;
        };
        let Ok((win, mut render_target, important)) =
            window_query.get_mut(match map.get(window_id) {
                None => continue,
                Some(v) => v,
            })
        else {
            continue;
        };
        if let WindowEvent::Resized(size) = event {
            render_target.set_size((size.width, size.height));
        } else if let WindowEvent::RedrawRequested = event {
            if let Err(e) = render_target.update(&device.0, &win.surface) {
                if e == SurfaceError::OutOfMemory {
                    eprintln!("Surface out of memory, exiting...");
                    commands.insert_resource(ShouldExit);
                } else {
                    win.window.request_redraw();
                }
            }
            if important {
                commands.insert_resource(ShouldDraw);
            }
        }
    }
}

fn create_surface_targets(
    mut commands: Commands,
    adapter: Res<AdapterRes>,
    format: Res<SurfaceFormat>,
    window_query: Query<(Entity, &WindowComponent), Without<SurfaceRenderTarget>>,
) {
    for (e, WindowComponent { window, surface }) in window_query.iter() {
        let mut rt = SurfaceRenderTarget::new(SurfaceRenderTargetConfig::default());
        rt.init(format.0, surface.get_capabilities(&adapter.0));
        let s = window.inner_size();
        rt.set_size((s.width, s.height));
        commands.entity(e).insert(rt);
    }
}

fn draw(world: &mut World) {
    world.run_schedule(PreSynchronize);
    world.run_schedule(Synchronize);
    world.run_schedule(PostSynchronize);
    world.run_schedule(PreDraw);
    world.run_schedule(Draw);
    world.run_schedule(PostDraw);
}

fn apply_offscreen_targets(
    device: Res<DeviceRes>,
    mut target_query: Query<&mut OffscreenRenderTarget>,
) {
    for mut rt in target_query.iter_mut() {
        rt.apply_changes(&device.0);
    }
}

fn present_surface_targets(mut target_query: Query<&mut SurfaceRenderTarget>) {
    for mut rt in target_query.iter_mut() {
        rt.present();
    }
}

fn request_redraws(query: Query<&WindowComponent, With<UpdatingWindow>>) {
    for WindowComponent { window, surface: _ } in query.iter() {
        window.request_redraw();
    }
}
