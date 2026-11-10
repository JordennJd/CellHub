//! A tileable, mipmapped grain shared by the substrate and cell materials.

const SIZE: u32 = 128;

fn noise(x: u32, y: u32, period: u32) -> f32 {
    let lattice = |x: u32, y: u32| {
        let mut hash =
            ((x % period) | ((y % period) << 8) | (period << 16)).wrapping_mul(0x45d9f3b);
        hash = (hash ^ (hash >> 16)).wrapping_mul(0x45d9f3b);
        ((hash ^ (hash >> 16)) & 255) as f32 / 255.
    };
    let spacing = SIZE / period;
    let (ix, iy) = (x / spacing, y / spacing);
    let smooth = |value| {
        let t = (value % spacing) as f32 / spacing as f32;
        t * t * (3. - 2. * t)
    };
    let mix = |a, b, t| a + (b - a) * t;
    let u = smooth(x);
    mix(
        mix(lattice(ix, iy), lattice(ix + 1, iy), u),
        mix(lattice(ix, iy + 1), lattice(ix + 1, iy + 1), u),
        smooth(y),
    )
}

pub(super) fn texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let levels = SIZE.ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("substrate and cell grain"),
        size: extent(SIZE),
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut pixels: Vec<_> = (0..SIZE)
        .flat_map(|y| {
            (0..SIZE).map(move |x| {
                (255. * (0.48 * noise(x, y, 4) + 0.34 * noise(x, y, 16) + 0.18 * noise(x, y, 64)))
                    .round() as u8
            })
        })
        .collect();
    for level in 0..levels {
        let size = SIZE >> level;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size),
                rows_per_image: Some(size),
            },
            extent(size),
        );
        if size > 1 {
            pixels = downsample(&pixels, size as usize);
        }
    }
    texture
}

fn extent(size: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: size,
        height: size,
        depth_or_array_layers: 1,
    }
}

fn downsample(pixels: &[u8], size: usize) -> Vec<u8> {
    (0..size)
        .step_by(2)
        .flat_map(|y| {
            (0..size).step_by(2).map(move |x| {
                let i = y * size + x;
                let sum = u16::from(pixels[i])
                    + u16::from(pixels[i + 1])
                    + u16::from(pixels[i + size])
                    + u16::from(pixels[i + size + 1]);
                ((sum + 2) / 4) as u8
            })
        })
        .collect()
}
