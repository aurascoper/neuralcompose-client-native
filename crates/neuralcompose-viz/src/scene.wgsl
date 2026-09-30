struct Uniform { mvp: mat4x4<f32>, params: vec4<f32> }
@group(0) @binding(0) var<uniform> u: Uniform;
struct Out { @builtin(position) pos: vec4<f32>, @location(0) color: vec4<f32>, @location(1) uv: vec2<f32> }
@vertex fn line_vertex(@location(0) channels: vec4<f32>, @location(1) life: f32) -> Out {
    var out: Out;
    out.pos = u.mvp * vec4<f32>(channels.y, channels.z, channels.x, 1.0);
    let energy=clamp((channels.w+1.0)*0.5,0.0,1.0);
    let rgb=mix(vec3<f32>(0.0,0.65,1.0),vec3<f32>(0.70,0.08,1.0),energy);
    out.color=vec4<f32>(rgb*pow(life,3.0)*u.params.x*1.4,1.0);
    out.uv=vec2<f32>(0.0);return out;
}
@fragment fn line_fragment(in:Out)->@location(0) vec4<f32>{return in.color;}
fn hash(x:f32)->f32{return fract(sin(x*127.1)*43758.5453);}
@vertex fn particle_vertex(@builtin(vertex_index) vertex:u32,@builtin(instance_index) instance:u32)->Out{
    let corners=array<vec2<f32>,6>(vec2<f32>(-1,-1),vec2<f32>(1,-1),vec2<f32>(1,1),vec2<f32>(-1,-1),vec2<f32>(1,1),vec2<f32>(-1,1));
    let id=f32(instance);let corner=corners[vertex];
    let center=vec2<f32>(hash(id+1.0),hash(id+531.0))*2.0-1.0;
    let drift=vec2<f32>(sin(u.params.z*0.09+id),cos(u.params.z*0.07+id))*0.025;
    var out:Out;out.pos=vec4<f32>(center+drift+corner*0.025,0.9,1.0);out.uv=corner;
    let ratio=u.params.y;let density=select(1.0-ratio,ratio,instance%2u==0u);
    let density_gate=select(0.0,1.0,hash(id+93.0)<density);
    out.color=vec4<f32>(mix(vec3<f32>(0.25,0.03,0.5),vec3<f32>(0.0,0.25,0.4),ratio)*density_gate*u.params.x*0.035,1.0);return out;
}
@fragment fn particle_fragment(in:Out)->@location(0) vec4<f32>{
    return vec4<f32>(in.color.rgb*exp(-dot(in.uv,in.uv)*4.0),1.0);
}
@group(0) @binding(0) var scene:texture_2d<f32>;
@group(0) @binding(1) var scene_sampler:sampler;
struct Screen { @builtin(position) pos:vec4<f32>,@location(0) uv:vec2<f32> }
@vertex fn screen_vertex(@builtin(vertex_index) i:u32)->Screen{
    let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));var o:Screen;o.pos=vec4<f32>(uv*2.0-1.0,0,1);o.uv=vec2<f32>(uv.x,1.0-uv.y);return o;
}
@fragment fn bloom_fragment(in:Screen)->@location(0) vec4<f32>{
    let pixel=1.0/vec2<f32>(textureDimensions(scene));
    var light=textureSample(scene,scene_sampler,in.uv).rgb;
    for(var x:i32=-1;x<=1;x++){for(var y:i32=-1;y<=1;y++){
        light+=textureSample(scene,scene_sampler,in.uv+vec2<f32>(f32(x),f32(y))*pixel*3.0).rgb*0.13;
    }}
    let rgb=pow(vec3<f32>(1.0)-exp(-light),vec3<f32>(1.0/2.2));
    return vec4<f32>(rgb,clamp(max(rgb.x,max(rgb.y,rgb.z)),0.0,1.0));
}
