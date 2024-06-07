use std::path::Path;

use bespoke_engine::resource_loader::generate_resources;

fn main() {
    generate_resources(Path::new("src/res"));
}