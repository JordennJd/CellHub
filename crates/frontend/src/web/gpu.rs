//! Small wgpu resource builders and the vertex layouts they share.

pub(super) const CELL_STRIDE: u64 = 40;
pub(super) const LINK_STRIDE: u64 = 24;
pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x2];
pub(super) const LINK_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2];

pub(super) fn buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    vertex: &str,
    fragment: &str,
    instance_layout: Option<(u64, &[wgpu::VertexAttribute])>,
) -> wgpu::RenderPipeline {
    let buffers = [wgpu::VertexBufferLayout {
        array_stride: instance_layout.map_or(CELL_STRIDE, |v| v.0),
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: instance_layout.map_or(&ATTRIBUTES[..], |v| v.1),
    }];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: if instance_layout.is_some() {
                &buffers
            } else {
                &[]
            },
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}
