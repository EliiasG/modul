use wgpu::PowerPreference;
use winit::window::WindowAttributes;
use modul_core::{run_app, DefaultGraphicsInitializer};
use modul_render::RenderPlugin;
use modul_util::ExitPlugin;

fn main() {
    run_app(DefaultGraphicsInitializer { 
        power_preference: PowerPreference::None,
        window_attribs: WindowAttributes::default(),
    }, |app| {
        app.add_plugins((RenderPlugin, ExitPlugin));
    });
}