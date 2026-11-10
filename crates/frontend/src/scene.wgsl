struct Camera { size: vec2f, center: vec2f, zoom: f32, layer: f32, selected: f32, padding: f32 }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var terrain: texture_2d<f32>;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;
@group(0) @binding(4) var material: texture_2d<f32>;
@group(0) @binding(5) var material_sampler: sampler;

fn corner(i:u32)->vec2f {
    let positions=array<vec2f,6>(vec2f(-1.,-1.),vec2f(1.,-1.),vec2f(-1.,1.),vec2f(-1.,1.),vec2f(1.,-1.),vec2f(1.,1.));
    return positions[i];
}
fn project(world:vec2f)->vec4f {let p=(world-camera.center)*camera.zoom;return vec4f(p.x*2./camera.size.x,-p.y*2./camera.size.y,0.,1.);}
fn rgb(v:u32)->vec3f {return vec3f(f32((v>>16u)&255u),f32((v>>8u)&255u),f32(v&255u))/255.;}
fn relative_luminance(color:vec3f)->f32 {
    let linear=select(pow((color+vec3f(0.055))/1.055,vec3f(2.4)),color/12.92,color<=vec3f(0.04045));
    return dot(linear,vec3f(0.2126,0.7152,0.0722));
}
// Premultiplied alpha compositing, matching the separate original shapes.
fn over(base:vec4f,color:vec3f,a:f32)->vec4f {return vec4f(color*a,a)+base*(1.-a);}
fn coverage(distance:f32)->f32 {let aa=max(fwidth(distance),0.001);return 1.-smoothstep(-aa*0.5,aa*0.5,distance);}
fn dish_distance(world:vec2f)->f32 {return length(world-vec2f(1536.))-1536.;}
fn grain(uv: vec2f) -> f32 {
    return textureSample(material, material_sampler, uv).r - 0.5;
}

struct TerrainOut {@builtin(position) pos:vec4f,@location(0) world:vec2f}
@vertex fn terrain_vs(@builtin(vertex_index) i:u32)->TerrainOut {
    let p=corner(i);var o:TerrainOut;o.pos=vec4f(p.x,-p.y,0.,1.);o.world=p*camera.size*0.5/camera.zoom+camera.center;return o;
}
@fragment fn terrain_fs(o:TerrainOut)->@location(0) vec4f {
    let w=o.world;
    let distance=dish_distance(w);
    let mask=coverage(distance);
    let ij=vec2u(clamp(w/64.,vec2f(0.),vec2f(47.)));let t=textureLoad(terrain,vec2i(ij),0);
    var color=vec4f(rgb(0x253e2bu),1.);
    if camera.layer==1. {color=vec4f(mix(rgb(0x253e2bu),rgb(0xe7cf7du),clamp(t.x*0.65,0.,1.)),1.);}
    if camera.layer==2. {color=vec4f(mix(rgb(0x253e2bu),rgb(0xd28ce9u),clamp(t.y*0.65,0.,1.)),1.);}
    if camera.layer==3. {color=vec4f(mix(rgb(0x253e2bu),rgb(0x75d5bdu),clamp(t.z*0.65,0.,1.)),1.);}
    if camera.layer==4. {color=vec4f(mix(rgb(0x253e2bu),rgb(0x88c4f0u),clamp(t.z*0.65,0.,1.)),1.);}
    // World coordinates keep the surface grain still as the camera moves.
    let mottling = grain(w / 224.) * select(0.34, 0.1, camera.layer > 0.);
    color = vec4f(clamp(color.rgb + mottling * vec3f(0.78, 1., 0.84), vec3f(0.), vec3f(1.)), 1.);
    let food_center=vec2f(ij)*64.+vec2f(17.+f32(ij.y%3u)*9.,27.+f32(ij.x%3u)*7.);
    // The center jumps at tile boundaries, so derive the pixel width from the continuous position.
    let food_edge = 0.5 * max(length(fwidth(w)), 0.001);
    let food = 1. - smoothstep(1.8 - food_edge, 1.8 + food_edge, length(w - food_center));
    if camera.layer!=4. && t.z>0.15 {color=over(color,rgb(0xa5cf9bu),food*t.z*0.35*smoothstep(0.2,0.65,camera.zoom));}
    color=over(color,rgb(0x7b9889u),coverage(abs(distance+0.65/camera.zoom)-0.65/camera.zoom));
    return color*mask;
}

struct LabelOut {@builtin(position) pos:vec4f,@location(0) uv:vec2f}
@vertex fn label_vs(@builtin(vertex_index) v:u32,@builtin(instance_index) i:u32)->LabelOut {
    let origins=array<vec2f,4>(vec2f(470.,650.),vec2f(1750.,1550.),vec2f(1250.,2600.),vec2f(2330.,680.));
    let uv=corner(v)*0.5+0.5;var o:LabelOut;o.pos=project(origins[i]+uv*vec2f(512.,32.));o.uv=vec2f(uv.x,(f32(i)+uv.y)*0.25);return o;
}
@fragment fn label_fs(o:LabelOut)->@location(0) vec4f {return vec4f(0.);}

struct CellOut {
    @builtin(position) pos:vec4f,
    @location(0) local:vec2f,
    @location(1) @interpolate(flat) radius:f32,
    @location(2) @interpolate(flat) kind:u32,
    @location(3) @interpolate(flat) angle:f32,
    @location(4) @interpolate(flat) mutations:f32,
    @location(5) @interpolate(flat) selected:u32,
    @location(6) @interpolate(flat) custom_color:f32,
    @location(7) world:vec2f,
    @location(8) @interpolate(flat) material_phase:vec2f,
    @location(9) @interpolate(flat) handedness:f32,
}
@vertex fn cell_vs(@builtin(vertex_index) v:u32,@location(0) a:vec4f,@location(1) b:vec4f,@location(2) c:vec4f)->CellOut {
    // id,x,y,radius, kind,mass,age,generation, mutations,angle,RGB,mirrored.
    var o:CellOut;
    let radius=a.w;
    let extent=max(radius*2.+2./camera.zoom,radius+9./camera.zoom);
    o.local=corner(v)*extent;
    o.world=a.yz+o.local;
    o.pos=project(o.world);
    o.radius=radius;
    o.kind=u32(b.x);
    o.angle=c.y;
    o.mutations=c.x;
    o.selected=u32(a.x==camera.selected);
    o.custom_color=c.z;
    o.material_phase = fract(vec2f(0.38197, 0.61803) * f32(u32(abs(a.x)) % 251u));
    o.handedness = select(1., -1., c.w > 0.);
    return o;
}
@fragment fn cell_fs(o:CellOut)->@location(0) vec4f {
    let palette=array<u32,7>(0xa4db71u,0xf1bf72u,0x71cbd4u,0xc997edu,0xb3c3d0u,0xf08373u,0x8ed6b3u);
    var color=rgb(palette[min(o.kind,6u)]);
    if o.custom_color>=0. {color=rgb(u32(o.custom_color));}
    let r=o.radius;let d=length(o.local);let direction=vec2f(cos(o.angle),sin(o.angle));
    let halo=coverage(d-r-1./camera.zoom);let body=coverage(d-r);let outline=coverage(abs(d-r)-0.6/camera.zoom);
    let nucleus=coverage(d-r*0.21);
    let mutation=coverage(length(o.local-vec2f(r*0.55,-r*0.55))-2.);
    let selection=coverage(abs(d-r-4./camera.zoom)-0.75/camera.zoom);
    // Type markings rotate and mirror with the cell, while the custom RGB
    // continues to tint the membrane and cytoplasm.
    let radial = o.local / max(r, 0.001);
    let side = vec2f(-direction.y, direction.x) * o.handedness;
    let p = vec2f(dot(radial, direction), dot(radial, side));
    let aa = clamp(0.5 * length(fwidth(radial)), 0.001, 0.5);
    let appendages = marking_coverage(appendage_distance(o.kind, p), aa);
    let patch_grain = grain(p * 0.68 + o.material_phase) * 0.17;
    let cytoplasm = 0.34 - 0.08 * min(dot(radial, radial), 1.);
    // Keep custom body RGB; lift only details that fall below 3:1 against the dish.
    var ink=color;
    let minimum_ink_luminance=3.*(relative_luminance(rgb(0x253e2bu))+0.05)-0.05;
    if o.custom_color>=0. && relative_luminance(color)<minimum_ink_luminance {ink=rgb(0xdce6dcu);}
    var out = over(vec4f(0.), color, halo * 0.08);
    out = over(out, color, body * cytoplasm);
    if o.kind == 0u {
        let chloroplasts = marking_coverage(chloroplast_distance(p) + patch_grain, aa);
        out = over(out, rgb(0x57a73fu), chloroplasts * body * 0.88);
    }
    if o.kind == 4u {
        let inner_membrane = marking_coverage(abs(length(radial) - 0.72) - 0.05, aa);
        out = over(out, ink, inner_membrane * 0.8);
    }
    out = over(out, ink, appendages);
    out = over(out, ink, outline);
    out = over(out, ink, nucleus * 0.9);
    if o.mutations>0. {out=over(out,vec3f(1.),mutation);}
    if o.selected!=0u {out=over(out,vec3f(1.),selection);}
    out*=coverage(dish_distance(o.world));
    if out.a<0.001 {discard;}return out;
}

struct LinkOut {
    @builtin(position) pos:vec4f,
    @location(0) local:vec2f,
    @location(1) @interpolate(flat) length:f32,
    @location(2) @interpolate(flat) selected:u32,
    @location(3) world:vec2f,
}
@vertex fn link_vs(@builtin(vertex_index) v:u32,@location(0) ends:vec4f,@location(1) ids:vec2f)->LinkOut {
    let delta=ends.zw-ends.xy;
    let len=length(delta);
    // A zero-length bond still produces a finite round cap.
    let direction=select(vec2f(1.,0.),delta/max(len,0.0001),len>0.0001);
    let normal=vec2f(-direction.y,direction.x);
    let selected=u32(ids.x==camera.selected || ids.y==camera.selected);
    let half_width=select(1.,1.5,selected!=0u);
    let extent=half_width+1.5/camera.zoom;
    let uv=corner(v);
    let local=vec2f((uv.x+1.)*0.5*(len+2.*extent)-extent,uv.y*extent);
    var o:LinkOut;o.world=ends.xy+direction*local.x+normal*local.y;o.pos=project(o.world);
    o.local=local;o.length=len;o.selected=selected;return o;
}
@fragment fn link_fs(o:LinkOut)->@location(0) vec4f {
    let half_width=select(1.,1.5,o.selected!=0u);
    let alpha=coverage(segment_distance(o.local,vec2f(0.),vec2f(o.length,0.))-max(half_width,0.6/camera.zoom))*0.8*coverage(dish_distance(o.world));
    let color=select(rgb(0xadc89du),rgb(0xe4f5c5u),o.selected!=0u);
    if alpha<0.001 {discard;}return vec4f(color*alpha,alpha);
}
