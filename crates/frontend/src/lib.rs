//! Rendering data is testable natively; the wgpu renderer runs in the browser
//! on WebGPU or its WebGL2 fallback.
#[cfg(any(target_arch = "wasm32", test))]
mod scene_data;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::WorldRenderer;

#[cfg(test)]
mod shader_tests {
    #[test]
    fn scene_shader_validates_with_dynamic_dish_size_and_ten_biomes() {
        let source = format!(
            "{}\n{}",
            include_str!("cell_shapes.wgsl"),
            include_str!("scene.wgsl")
        );
        let module = naga::front::wgsl::parse_str(&source).expect("valid WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("portable scene shader");
    }
}
