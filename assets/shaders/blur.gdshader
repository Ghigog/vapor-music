shader_type canvas_item;
uniform sampler2D screen_texture : hint_screen_texture, filter_linear_mipmap;
uniform float blur_amount : hint_range(0.0, 10.0) = 2.0;

void fragment() {
    COLOR = textureLod(screen_texture, SCREEN_UV, blur_amount);
    // Dark/tint overlay to make the text on top pop and be readable
    COLOR.rgb = mix(COLOR.rgb, vec3(0.0), 0.3);
}
