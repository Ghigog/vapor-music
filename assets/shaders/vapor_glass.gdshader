shader_type canvas_item;

uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

group_uniforms Blur;
uniform float blur_amount : hint_range(0.0, 5.0, 0.1) = 2.5;

group_uniforms Grain;
uniform float grain_amount : hint_range(0.0, 0.2, 0.01) = 0.03;

group_uniforms Highlight;
uniform float edge_highlight : hint_range(0.0, 1.0, 0.05) = 0.15;
uniform float fallback_opacity : hint_range(0.0, 1.0, 0.05) = 0.85;

// Pseudo-random noise function for the frosted grain
float rand(vec2 co) {
	return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453);
}

void fragment() {
	// Sample the screen behind this Control with mipmap-based blur
	vec4 blur_color = textureLod(SCREEN_TEXTURE, SCREEN_UV, blur_amount);
	
	// If screen texture is empty or dark, blend with a fallback opacity
	vec4 base_color = COLOR;
	
	// Apply the blur mixed with the stylebox background color (tint)
	vec4 glass_color = mix(blur_color, vec4(base_color.rgb, 1.0), base_color.a);
	
	// Add a subtle frosted glass grain/noise
	float grain = (rand(SCREEN_UV * 100.0) - 0.5) * grain_amount;
	glass_color.rgb = clamp(glass_color.rgb + grain, 0.0, 1.0);
	
	// Generate an elegant top-left highlight border simulation
	// UV coordinates go from (0,0) top-left to (1,1) bottom-right
	float top_border = smoothstep(0.03, 0.0, UV.y);
	float left_border = smoothstep(0.03, 0.0, UV.x);
	float highlight = max(top_border, left_border) * edge_highlight * base_color.a;
	
	// Final output color
	COLOR = vec4(glass_color.rgb + vec3(highlight), base_color.a);
}
