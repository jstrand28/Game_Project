mod game;
mod maze;
mod items;
mod stats;
mod inventory;
mod ui;
mod util;

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
                    resolution: (1280.0, 720.0).into(),
                    ..default()
                }),
                ..default()
            })
        )
        // Third-party plugins next
        .add_plugins(EguiPlugin)
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(RapierDebugRenderPlugin::default())
        // Your game plugin last
        .add_plugins(GamePlugin)
        .run();
}