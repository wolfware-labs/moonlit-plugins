mod manifest;
mod publish_plugin;
mod run_modules;

use moonlit_pdk::prelude::*;

use publish_plugin::PublishPlugin;
use run_modules::RunModules;

moonlit_plugin! {
    name: "moonlit",
    icon: "icon.png",
    middlewares: [RunModules, PublishPlugin],
}
