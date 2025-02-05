use bevy_app::{PluginsState, SubApp};
use bevy_ecs::entity::EntityHashMap;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;
use std::collections::HashMap;
use std::mem;
use std::sync::Arc;
use wgpu::{
    Adapter, Backends, Device, DeviceDescriptor, Instance, InstanceDescriptor, PowerPreference,
    PresentMode, Queue, RequestAdapterOptions, Surface, SurfaceConfiguration, TextureFormat,
};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, Event, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

/// Not using apps, so instead of a runner you should pass a [SubApp] to this
pub fn run_app(graphics_initializer: impl GraphicsInitializer, setup: impl FnOnce(&mut SubApp)) {
    let mut app = SubApp::new();
    app.init_schedule(PreInit);
    app.init_schedule(Init);
    app.init_schedule(Redraw);
    app.insert_resource(WindowRequests(Vec::new()));
    app.insert_resource(CreatedWindows(Vec::new()));

    app.update_schedule = Some(Redraw.intern());
    app.add_systems(
        Redraw,
        (
            window_insert_system,
            window_request_system,
            window_map_removal,
        ),
    );

    setup(&mut app);
    // perhaps there is a better way to do this?
    while app.plugins_state() == PluginsState::Adding {}
    app.finish();
    app.cleanup();

    let world = app.world_mut();
    world.run_schedule(PreInit);
    world.clear_trackers();
    EventLoop::new()
        .expect("failed to build event loop")
        .run_app(&mut WinitApp {
            app,
            initializer: Some(graphics_initializer),
            buffer: EventBuffer(Vec::new()),
        })
        .expect("unable to run event loop");
}

/// Runs before WGPU and winit are set up, for loading stuff before the window appears
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct PreInit;

/// Runs after WGPU and winit are set up
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Init;

/// The main schedule of the app, will be run whenever a window requests redraw
#[derive(ScheduleLabel, Clone, Hash, PartialEq, Eq, Debug)]
pub struct Redraw;

/// Contains the events accumulated since last run of [Redraw]
#[derive(Resource)]
pub struct EventBuffer(Vec<Event<()>>);

impl EventBuffer {
    pub fn events(&self) -> &[Event<()>] {
        self.0.as_ref()
    }
}

/// If this exists after running [Redraw], the app will exit
#[derive(Resource)]
pub struct ShouldExit;

#[derive(Resource)]
pub struct InstanceRes(pub Instance);

#[derive(Resource)]
pub struct DefaultSurfaceConfig(pub SurfaceConfiguration);

#[derive(Resource)]
pub struct AdapterRes(pub Adapter);

#[derive(Resource)]
pub struct DeviceRes(pub Device);

#[derive(Resource)]
pub struct QueueRes(pub Queue);

/// Preferred format for surfaces
#[derive(Resource)]
pub struct SurfaceFormat(pub TextureFormat);

/// Map from [WindowId] to [Entity]
#[derive(Resource)]
pub struct WindowMap {
    map: HashMap<WindowId, Entity>,
    // not exposed publicly, as the id can be gotten from the window in the component, this is just used for removing items from the map
    inverse: EntityHashMap<WindowId>,
}

impl WindowMap {
    pub fn get(&self, window: &WindowId) -> Option<Entity> {
        self.map.get(window).copied()
    }
}

/// Marker for the main window
#[derive(Component)]
pub struct MainWindow;

/// This indicates that an extraction and draw should happen when the window requests a redraw, and that a redraw will be requested immediately after redrawing to it.
/// This is automatically added to the main window, but may be removed.
#[derive(Component)]
pub struct ImportantWindow;

/// This indicates that a window should request a redraw every frame
#[derive(Component)]
pub struct UpdatingWindow;

#[derive(Component)]
pub struct WindowComponent {
    pub window: Arc<Window>,
    pub surface: Surface<'static>,
}

/// Holds the initial surface configuration of a [WindowComponent], this should be added to open a window, but not updated during the life of the window
#[derive(Component)]
pub struct InitialWindowConfig {
    pub window_attribs: WindowAttributes,
}

impl InitialWindowConfig {
    pub fn window_attribs(&self) -> &WindowAttributes {
        &self.window_attribs
    }
}

pub struct GraphicsInitializerResult {
    pub window: Arc<Window>,
    pub surface: Surface<'static>,
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub window_attribs: WindowAttributes,
    pub surface_format: TextureFormat,
}

pub trait GraphicsInitializer: Send + Sync + 'static {
    fn initialize(self, event_loop: &ActiveEventLoop) -> GraphicsInitializerResult;
}

pub struct DefaultGraphicsInitializer {
    pub power_preference: PowerPreference,
    pub window_attribs: WindowAttributes,
}

impl GraphicsInitializer for DefaultGraphicsInitializer {
    fn initialize(self, event_loop: &ActiveEventLoop) -> GraphicsInitializerResult {
        env_logger::init();
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let window = event_loop
            .create_window(self.window_attribs.clone())
            .expect("failed to create window");
        // must be static because it has to be a bevy resource
        let window = Arc::new(window);

        let surface = instance
            .create_surface(window.clone())
            .expect("no surface?");

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: self.power_preference,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .expect("no adapter?");

        let (device, queue) =
            pollster::block_on(adapter.request_device(&DeviceDescriptor::default(), None))
                .expect("no device?");

        let surface_format = surface
            .get_capabilities(&adapter)
            .formats
            .iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
            .expect("SRGB not supported, this is strange...");
        GraphicsInitializerResult {
            window,
            surface,
            instance,
            adapter,
            device,
            queue,
            window_attribs: self.window_attribs,
            surface_format,
        }
    }
}

struct WinitApp<I: GraphicsInitializer> {
    app: SubApp,
    initializer: Option<I>,
    buffer: EventBuffer,
}

impl<I: GraphicsInitializer> ApplicationHandler for WinitApp<I> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        self.buffer.0.push(Event::NewEvents(cause));
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.buffer.0.push(Event::Resumed);
        let Some(init) = self.initializer.take() else {
            return;
        };
        let res = init.initialize(event_loop);
        add_resources(self.app.world_mut(), res);
        self.app.world_mut().run_schedule(Init);
        self.app.world_mut().clear_trackers();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // stupid, but to not clone event
        if let WindowEvent::RedrawRequested = event {
            self.buffer.0.push(Event::WindowEvent { window_id, event });
            self.app
                .insert_resource(mem::replace(&mut self.buffer, EventBuffer(Vec::new())));
            self.app.update();
            if self.app.world().contains_resource::<ShouldExit>() {
                event_loop.exit();
                return;
            }
            self.app
                .world_mut()
                .resource_scope(|world, mut cw: Mut<CreatedWindows>| {
                    world.resource_scope(|world, instance: Mut<InstanceRes>| {
                        for (entity, window_attribs) in
                            world.resource_mut::<WindowRequests>().0.drain(..)
                        {
                            let window = Arc::new(
                                event_loop
                                    .create_window(window_attribs)
                                    .expect("failed to create window"),
                            );
                            let surface = instance
                                .0
                                .create_surface(window.clone())
                                .expect("no surface?");
                            cw.0.push((entity, WindowComponent { window, surface }))
                        }
                    });
                });
        } else {
            self.buffer.0.push(Event::WindowEvent { window_id, event });
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.buffer.0.push(Event::DeviceEvent { device_id, event });
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.buffer.0.push(Event::AboutToWait);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.buffer.0.push(Event::Suspended);
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        self.buffer.0.push(Event::MemoryWarning);
    }
}

fn add_resources(world: &mut World, init_res: GraphicsInitializerResult) {
    let id = init_res.window.id();
    world.insert_resource(InstanceRes(init_res.instance));
    world.insert_resource(AdapterRes(init_res.adapter));
    world.insert_resource(DeviceRes(init_res.device));
    world.insert_resource(QueueRes(init_res.queue));
    world.insert_resource(SurfaceFormat(init_res.surface_format));
    let e = world.spawn((
        WindowComponent {
            window: init_res.window,
            surface: init_res.surface,
        },
        InitialWindowConfig {
            window_attribs: init_res.window_attribs,
        },
        MainWindow,
        ImportantWindow,
        UpdatingWindow,
    )).id();
    let mut inverse = EntityHashMap::default();
    inverse.insert(e, id);
    world.insert_resource(WindowMap {
        map: [(id, e)].into(),
        inverse,
    });
}

#[derive(Resource)]
struct WindowRequests(Vec<(Entity, WindowAttributes)>);

#[derive(Resource)]
struct CreatedWindows(Vec<(Entity, WindowComponent)>);

fn window_request_system(
    mut window_requests: ResMut<WindowRequests>,
    new_windows: Query<(Entity, &InitialWindowConfig), Without<WindowComponent>>,
) {
    for (entity, cfg) in new_windows.iter() {
        window_requests.0.push((entity, cfg.window_attribs.clone()));
    }
}

fn window_insert_system(
    mut commands: Commands,
    mut windows: ResMut<CreatedWindows>,
    mut window_map: ResMut<WindowMap>,
) {
    for (entity, comp) in windows.0.drain(..) {
        window_map.map.insert(comp.window.id(), entity);
        window_map.inverse.insert(entity, comp.window.id());
        commands.entity(entity).insert(comp);
    }
}

fn window_map_removal(
    mut removed: RemovedComponents<WindowComponent>,
    mut window_map: ResMut<WindowMap>,
) {
    for ent in removed.read() {
        let Some(win_id) = window_map.inverse.remove(&ent) else {
            continue;
        };
        window_map
            .map
            .remove(&win_id)
            .expect("in inverse but not main");
    }
}
