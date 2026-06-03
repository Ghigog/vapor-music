shader_type canvas_item;
uniform sampler2D screen_texture : hint_screen_texture, filter_linear_mipmap;
uniform float blur_amount : hint_range(0.0, 10.0) = 2.0;

void fragment() {
    vec4 sampled = textureLod(screen_texture, SCREEN_UV, blur_amount);
    COLOR.rgb = mix(sampled.rgb, vec3(0.0), 0.45);
    COLOR.a = max(sampled.a, 0.5);
}
