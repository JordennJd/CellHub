struct Camera { size: vec2f, center: vec2f, zoom: f32, layer: f32, selected: f32, padding: f32 }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var terrain: texture_2d<f32>;
@group(0) @binding(2) var atlas: texture_2d<f32>;
@group(0) @binding(3) var atlas_sampler: sampler;

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
    let food_center=vec2f(ij)*64.+vec2f(17.+f32(ij.y%3u)*9.,27.+f32(ij.x%3u)*7.);
    let food=coverage(length(w-food_center)-1.8);
    if t.z>0.15 {color=over(color,rgb(0xa5cf9bu),food*t.z*0.35*smoothstep(0.2,0.65,camera.zoom));}
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
}
@vertex fn cell_vs(@builtin(vertex_index) v:u32,@location(0) a:vec4f,@location(1) b:vec4f,@location(2) c:vec4f)->CellOut {
    // id,x,y,radius, kind,mass,age,generation, mutations,angle,RGB,mirrored.
    var o:CellOut;let radius=a.w;let extent=max(radius*2.+2./camera.zoom,radius+9./camera.zoom);o.local=corner(v)*extent;
    o.world=a.yz+o.local;o.pos=project(o.world);o.radius=radius;o.kind=u32(b.x);o.angle=c.y;o.mutations=c.x;o.selected=u32(a.x==camera.selected);o.custom_color=c.z;return o;
}
fn segment_distance(p:vec2f,a:vec2f,b:vec2f)->f32 {let v=b-a;let t=clamp(dot(p-a,v)/max(dot(v,v),0.0001),0.,1.);return length(p-a-v*t);}
fn disc(p:vec2f,c:vec2f,radius:f32)->f32 {return coverage(length(p-c)-radius);}
fn ring(p:vec2f,c:vec2f,radius:f32,width:f32)->f32 {return coverage(abs(length(p-c)-radius)-width);}
fn stroke(p:vec2f,a:vec2f,b:vec2f,width:f32)->f32 {return coverage(segment_distance(p,a,b)-width);}
@fragment fn cell_fs(o:CellOut)->@location(0) vec4f {
    // Cell Lab's roster: every type keeps a distinct hue and organelle pattern.
    let palette=array<u32,18>(
        0xa4db71u,0xf1bf72u,0x71cbd4u,0xc997edu,0xb3c3d0u,0xf08373u,
        0xe8d17au,0x8fd8c0u,0xd4a6e8u,0x7bb98au,0xe6a26bu,0x9cc7e8u,
        0xd98c9cu,0xc9b3e8u,0xa8c98au,0xe3b52eu,0xf2a6c4u,0x8ed0a1u);
    var color=rgb(palette[min(o.kind,17u)]);
    if o.custom_color>=0. {color=rgb(u32(o.custom_color));}
    let r=o.radius;let local=o.local;let d=length(local);
    let direction=vec2f(cos(o.angle),sin(o.angle));
    let side=vec2f(-direction.y,direction.x);
    let width=max(r*0.045,0.8/camera.zoom);
    let halo=coverage(d-r-1./camera.zoom);let body=coverage(d-r);let outline=coverage(abs(d-r)-0.6/camera.zoom);
    let selection=coverage(abs(d-r-4./camera.zoom)-0.75/camera.zoom);
    // Keep custom body RGB; lift only details that fall below 3:1 against the dish.
    var ink=color;
    let minimum_ink_luminance=3.*(relative_luminance(rgb(0x253e2bu))+0.05)-0.05;
    if o.custom_color>=0. && relative_luminance(color)<minimum_ink_luminance {ink=rgb(0xdce6dcu);}
    let accent=mix(ink,vec3f(1.),0.45);
    var out=over(vec4f(0.),color,halo*0.08);out=over(out,color,body*0.22);out=over(out,ink,outline);
    // A faint shared nucleus anchors every type; the organelle below tells them apart.
    out=over(out,ink,coverage(length(local-direction*r*0.18)-r*0.28)*0.28);
    if o.kind==0u {
        // Photocyte: chloroplasts in a ring.
        for (var i=0;i<5;i++) {
            let a=f32(i)*1.256637+0.35;
            let p=vec2f(cos(a),sin(a))*r*0.5;
            let size=max(r*0.11,1.2/camera.zoom);
            out=over(out,accent,disc(local,p,size)*0.55);
            out=over(out,ink,ring(local,p,size,width*0.45)*0.8);
        }
    }
    if o.kind==1u {
        // Phagocyte: food vacuoles and a feeding mouth.
        for (var i=0;i<3;i++) {
            let a=f32(i)*2.094395+0.5;
            let p=direction*r*0.32+vec2f(cos(a),sin(a))*r*0.35;
            out=over(out,accent,disc(local,p,r*0.16)*0.32);
            out=over(out,ink,ring(local,p,r*0.16,width*0.7)*0.85);
        }
        out=over(out,ink,stroke(local,direction*r*0.88+side*r*0.3,direction*r*0.88-side*r*0.3,width)*0.9);
    }
    if o.kind==2u {
        // Flagellocyte: a two-joint flagellum trails the body.
        let base=-direction*r;
        let middle=-direction*r*1.45+side*r*0.22;
        let tip=-direction*r*1.9-side*r*0.12;
        out=over(out,ink,stroke(local,base,middle,max(width,0.7/camera.zoom))*0.95);
        out=over(out,ink,stroke(local,middle,tip,max(width,0.7/camera.zoom))*0.95);
        out=over(out,accent,disc(local,base,r*0.11)*0.6);
    }
    if o.kind==3u {
        // Virocyte: a crown of infectivity spikes.
        for (var i=0;i<8;i++) {
            let a=f32(i)*0.785398;
            let dirv=direction*cos(a)+side*sin(a);
            out=over(out,ink,stroke(local,dirv*r*0.94,dirv*r*1.22,width*0.8)*0.85);
        }
        out=over(out,ink,ring(local,vec2f(0.),r*0.56,width*0.5)*0.5);
    }
    if o.kind==4u {
        // Keratinocyte: a thick armour shell and inner plates.
        out=over(out,ink,ring(local,vec2f(0.),r*0.72,width)*0.8);
        out=over(out,ink,ring(local,vec2f(0.),r+2.4/camera.zoom,0.8/camera.zoom)*0.55);
        for (var i=0;i<6;i++) {
            let a=f32(i)*1.047198+0.52;
            out=over(out,ink,disc(local,vec2f(cos(a),sin(a))*r*0.72,max(r*0.06,0.9/camera.zoom))*0.7);
        }
    }
    if o.kind==5u {
        // Devorocyte: a wide jaw and a digesting stomach.
        out=over(out,ink,stroke(local,direction*r*0.25+side*r*0.42,direction*r*0.82,width)*0.95);
        out=over(out,ink,stroke(local,direction*r*0.25-side*r*0.42,direction*r*0.82,width)*0.95);
        out=over(out,ink,ring(local,vec2f(0.),r*0.55,width*0.65)*0.6);
    }
    if o.kind==6u {
        // Lipocyte: one oversized fat vacuole.
        let p=-direction*r*0.08;
        out=over(out,accent,disc(local,p,r*0.55)*0.25);
        out=over(out,ink,ring(local,p,r*0.55,width)*0.9);
        out=over(out,accent,disc(local,p-direction*r*0.18,r*0.12)*0.35);
    }
    if o.kind==7u {
        // Buoyocyte: a gas vesicle with a highlight.
        out=over(out,accent,disc(local,vec2f(0.),r*0.46)*0.3);
        out=over(out,ink,ring(local,vec2f(0.),r*0.46,width)*0.9);
        out=over(out,accent,disc(local,vec2f(-r*0.14,r*0.16),r*0.1)*0.5);
    }
    if o.kind==8u {
        // Glueocyte: adhesive knobs around the membrane.
        for (var i=0;i<6;i++) {
            let a=f32(i)*1.047198;
            let dirv=direction*cos(a)+side*sin(a);
            out=over(out,ink,disc(local,dirv*r*1.08,max(r*0.09,1./camera.zoom))*0.9);
        }
        out=over(out,ink,ring(local,vec2f(0.),r*0.5,width*0.55)*0.45);
    }
    if o.kind==9u {
        // Nitrocyte: nitrogen-fixing nodules.
        for (var i=0;i<4;i++) {
            let a=f32(i)*1.570796+0.785398;
            out=over(out,ink,disc(local,vec2f(cos(a),sin(a))*r*0.45,r*0.1)*0.85);
        }
        out=over(out,ink,ring(local,vec2f(0.),r*0.62,width*0.45)*0.45);
    }
    if o.kind==10u {
        // Stereocyte: two directional eyes wired to the centre.
        let eye_a=direction*r*0.4+side*r*0.42;
        let eye_b=direction*r*0.4-side*r*0.42;
        out=over(out,accent,disc(local,eye_a,r*0.15)*0.4);
        out=over(out,ink,ring(local,eye_a,r*0.15,width*0.75)*0.9);
        out=over(out,accent,disc(local,eye_b,r*0.15)*0.4);
        out=over(out,ink,ring(local,eye_b,r*0.15,width*0.75)*0.9);
        out=over(out,ink,stroke(local,direction*r*0.15,eye_a,width*0.45)*0.5);
        out=over(out,ink,stroke(local,direction*r*0.15,eye_b,width*0.45)*0.5);
    }
    if o.kind==11u {
        // Senseocyte: a single lens with a receiving ring.
        let p=direction*r*0.42;
        out=over(out,accent,disc(local,p,r*0.2)*0.3);
        out=over(out,ink,ring(local,p,r*0.2,width*0.75)*0.9);
        out=over(out,ink,disc(local,p,r*0.07)*0.9);
        out=over(out,ink,ring(local,p,r*0.34,width*0.45)*0.45);
    }
    if o.kind==12u {
        // Myocyte: parallel contractile fibres.
        for (var i:i32=-2;i<=2;i=i+1) {
            let offset=f32(i)*r*0.22;
            out=over(out,ink,stroke(local,-direction*r*0.55+side*offset,direction*r*0.55+side*offset,width*0.55)*0.8);
        }
    }
    if o.kind==13u {
        // Neurocyte: soma, axon and two terminal branches.
        let start=-direction*r*0.05;
        let end=-direction*r*1.22;
        out=over(out,ink,stroke(local,start,end,max(width,0.8/camera.zoom))*0.9);
        out=over(out,ink,stroke(local,end,end+side*r*0.3-direction*r*0.18,width*0.65)*0.9);
        out=over(out,ink,stroke(local,end,end-side*r*0.3-direction*r*0.18,width*0.65)*0.9);
        out=over(out,accent,disc(local,direction*r*0.1,r*0.18)*0.4);
    }
    if o.kind==14u {
        // Secrocyte: secretory vesicles feeding a duct.
        for (var i=0;i<3;i++) {
            let a=f32(i)*2.094395+0.5;
            let p=direction*r*0.3+vec2f(cos(a),sin(a))*r*0.3;
            out=over(out,accent,disc(local,p,r*0.13)*0.45);
            out=over(out,ink,ring(local,p,r*0.13,width*0.65)*0.85);
        }
        out=over(out,ink,stroke(local,direction*r*0.55,direction*r*0.9,width*0.55)*0.7);
    }
    if o.kind==15u {
        // Stemocyte: a large, pale nucleus ready to differentiate.
        out=over(out,accent,disc(local,vec2f(0.),r*0.38)*0.28);
        out=over(out,ink,ring(local,vec2f(0.),r*0.38,width)*0.9);
        out=over(out,ink,disc(local,-direction*r*0.08,r*0.16)*0.8);
    }
    if o.kind==16u {
        // Gamete: a hooked mating tail and a pronucleus.
        out=over(out,ink,stroke(local,-direction*r*0.7,-direction*r*1.35+side*r*0.42,max(width,0.7/camera.zoom))*0.9);
        out=over(out,accent,disc(local,direction*r*0.25,r*0.14)*0.5);
        out=over(out,ink,ring(local,vec2f(0.),r*0.6,width*0.45)*0.4);
    }
    if o.kind==17u {
        // Ciliocyte: a comb of beating cilia around the rim.
        for (var i=0;i<12;i++) {
            let a=f32(i)*0.523599;
            let dirv=direction*cos(a)+side*sin(a);
            out=over(out,ink,stroke(local,dirv*r,dirv*r*1.18,width*0.65)*0.85);
        }
        out=over(out,ink,ring(local,vec2f(0.),r*0.55,width*0.45)*0.45);
    }
    let mutation=coverage(length(local-vec2f(r*0.55,-r*0.55))-2.);
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
