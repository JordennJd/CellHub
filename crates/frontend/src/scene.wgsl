struct Camera { size: vec2f, center: vec2f, zoom: f32, layer: f32, selected: f32, padding: f32 }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage,read> terrain: array<vec4f>;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;

fn corner(i:u32)->vec2f {
    let positions=array<vec2f,6>(vec2f(-1.,-1.),vec2f(1.,-1.),vec2f(-1.,1.),vec2f(-1.,1.),vec2f(1.,-1.),vec2f(1.,1.));
    return positions[i];
}
fn project(world:vec2f)->vec4f {let p=(world-camera.center)*camera.zoom;return vec4f(p.x*2./camera.size.x,-p.y*2./camera.size.y,0.,1.);}
fn rgb(v:u32)->vec3f {return vec3f(f32((v>>16u)&255u),f32((v>>8u)&255u),f32(v&255u))/255.;}
// Premultiplied alpha compositing, matching the separate original shapes.
fn over(base:vec4f,color:vec3f,a:f32)->vec4f {return vec4f(color*a,a)+base*(1.-a);}
fn coverage(distance:f32)->f32 {let aa=max(fwidth(distance),0.001);return 1.-smoothstep(-aa*0.5,aa*0.5,distance);}

struct TerrainOut {@builtin(position) pos:vec4f,@location(0) world:vec2f}
@vertex fn terrain_vs(@builtin(vertex_index) i:u32)->TerrainOut {
    let p=corner(i);var o:TerrainOut;o.pos=vec4f(p.x,-p.y,0.,1.);o.world=p*camera.size*0.5/camera.zoom+camera.center;return o;
}
@fragment fn terrain_fs(o:TerrainOut)->@location(0) vec4f {
    let w=o.world;
    let ij=min(vec2u(w/64.),vec2u(47u));let t=terrain[ij.y*48u+ij.x];
    let palette=array<u32,4>(0x253e2bu,0x233c38u,0x152d39u,0x3c2940u);
    var color=vec4f(rgb(palette[min(u32(t.w),3u)]),1.);
    if camera.layer==1. {let a=0.08+t.x*0.72;color=vec4f(rgb(0xe7cf7du)*a,a);}
    if camera.layer==2. {let a=0.08+t.y*0.72;color=vec4f(rgb(0xd28ce9u)*a,a);}
    if camera.layer==3. {let a=0.08+t.z*0.72;color=vec4f(rgb(0x75d5bdu)*a,a);}
    let food_center=vec2f(ij)*64.+vec2f(17.+f32(ij.y%3u)*9.,27.+f32(ij.x%3u)*7.);
    let food=coverage(length(w-food_center)-1.8);
    if t.z>0.15 {color=over(color,rgb(0xa5cf9bu),food*t.z*0.5);}
    let edge=min(min(w.x,w.y),min(3072.-w.x,3072.-w.y));
    color=over(color,rgb(0x7b9889u),coverage(edge-1.));
    if any(w<vec2f(0.)) || any(w>vec2f(3072.)) {discard;}
    return color;
}

struct LabelOut {@builtin(position) pos:vec4f,@location(0) uv:vec2f}
@vertex fn label_vs(@builtin(vertex_index) v:u32,@builtin(instance_index) i:u32)->LabelOut {
    let origins=array<vec2f,4>(vec2f(470.,650.),vec2f(1750.,1550.),vec2f(1250.,2600.),vec2f(2330.,680.));
    let uv=corner(v)*0.5+0.5;var o:LabelOut;o.pos=project(origins[i]+uv*vec2f(512.,32.));o.uv=vec2f(uv.x,(f32(i)+uv.y)*0.25);return o;
}
@fragment fn label_fs(o:LabelOut)->@location(0) vec4f {let a=textureSample(atlas,atlas_sampler,o.uv).a*0.5;return vec4f(rgb(0xa4bbaeu)*a,a);}

struct CellOut {
    @builtin(position) pos:vec4f,
    @location(0) local:vec2f,
    @location(1) @interpolate(flat) radius:f32,
    @location(2) @interpolate(flat) kind:u32,
    @location(3) @interpolate(flat) angle:f32,
    @location(4) @interpolate(flat) mutations:f32,
    @location(5) @interpolate(flat) selected:u32,
}
@vertex fn cell_vs(@builtin(vertex_index) v:u32,@location(0) a:vec4f,@location(1) b:vec4f,@location(2) c:vec2f)->CellOut {
    // Original snapshot: id,x,y,radius, kind,mass,age,generation, mutations,angle.
    var o:CellOut;let extent=max(a.w*2.+2.,a.w+9.);o.local=corner(v)*extent;
    o.pos=project(a.yz+o.local);o.radius=a.w;o.kind=u32(b.x);o.angle=c.y;o.mutations=c.x;o.selected=u32(a.x==camera.selected);return o;
}
fn segment_distance(p:vec2f,a:vec2f,b:vec2f)->f32 {let v=b-a;let t=clamp(dot(p-a,v)/max(dot(v,v),0.0001),0.,1.);return length(p-a-v*t);}
@fragment fn cell_fs(o:CellOut)->@location(0) vec4f {
    let palette=array<u32,6>(0xa4db71u,0xf1bf72u,0x71cbd4u,0xc997edu,0xb3c3d0u,0xf08373u);
    let color=rgb(palette[min(o.kind,5u)]);let r=o.radius;let d=length(o.local);let direction=vec2f(cos(o.angle),sin(o.angle));
    let halo=coverage(d-r-3.);let body=coverage(d-r);let outline=coverage(abs(d-r)-0.7);
    let nucleus=coverage(length(o.local-direction*r*0.2)-r*0.29);
    let tail=coverage(segment_distance(o.local,-direction*r,-direction*r*2.)-0.5);
    let mutation=coverage(length(o.local-vec2f(r*0.55,-r*0.55))-2.);
    let selection=coverage(abs(d-r-7.)-0.75);
    var out=over(vec4f(0.),color,halo*0.08);out=over(out,color,body*0.22);out=over(out,color,outline);out=over(out,color,nucleus*0.9);
    if o.kind==2u {out=over(out,color,tail);}
    if o.mutations>0. {out=over(out,vec3f(1.),mutation);}
    if o.selected!=0u {out=over(out,vec3f(1.),selection);}
    if out.a<0.001 {discard;}return out;
}
