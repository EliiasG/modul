use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
pub use hashbrown;
use modul_core::{EventBuffer, MainWindow, Redraw, ShouldExit, WindowMap};
use std::ops::{Deref, DerefMut, Range};
use winit::event::{Event, WindowEvent};

pub type HashMap<K, V> = hashbrown::HashMap<K, V>;

pub type HashSet<T> = hashbrown::HashSet<T>;

/// Plugin that will make the app close when the main window is requested to close
pub struct ExitPlugin;

impl Plugin for ExitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Redraw,
            |mut commands: Commands,
             events: Res<EventBuffer>,
             window_map: Res<WindowMap>,
             main: Query<Entity, With<MainWindow>>| {
                for e in events.events() {
                    if let Event::WindowEvent {
                        window_id,
                        event: WindowEvent::CloseRequested,
                    } = e
                    {
                        let e = window_map.get(window_id).unwrap();
                        if e == main.single() {
                            commands.insert_resource(ShouldExit);
                        }
                    }
                }
            },
        );
    }
}

/// Binary searches between lower and upper, returning the lowest value giving ok, if all values give error, the error returned by the end of the range is returned
pub fn binsearch<T, E>(
    mut f: impl FnMut(i32) -> Result<T, E>,
    range: impl Into<Range<i32>>,
) -> Result<T, E> {
    let mut range = range.into();
    if range.is_empty() {
        panic!("binsearch on empty range");
    }
    let mut res = None;
    while range.start < range.end {
        let mid = (range.start + range.end) / 2;
        res = Some(f(mid));
        if res.as_ref().unwrap().is_ok() {
            range.end = mid;
        } else {
            range.start = mid + 1;
        }
    }
    res.unwrap()
}
