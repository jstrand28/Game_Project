mod game;
mod maze;
mod items;
mod stats;
mod inventory;
mod ui;
mod util;
mod persistence;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_rapier3d::prelude::*;
use game::GamePlugin;

fn main() {
    App::new()
        // Window + renderer first
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Arc Maze".into(),
                    resolution: bevy::window::WindowResolution::new(1280, 720),
                    ..default()
                }),
                ..default()
            })
        )
        // Third-party plugins next
        .add_plugins(EguiPlugin::default())
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        // Your game plugin last
        .add_plugins(GamePlugin)
        .run();
}