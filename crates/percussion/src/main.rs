//! Thin binary entry point. All real work happens in the `percussion`
//! library crate so it can be reused and tested independently.

use bevy::prelude::*;
use percussion::GamePlugin;

fn main() {
    App::new().add_plugins(GamePlugin).run();
}
