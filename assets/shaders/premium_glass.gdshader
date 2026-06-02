// premium_glass.gdshader
// High-fidelity Apple-style glassmorphism shader for Godot 4.
// Features SDF-based rounded corners, anti-aliased dual-stroke borders (highlight/shadow),
// fine noise grain, and backdrop saturation boost.

shader_type canvas_item;
render_mode blend_mix;

// Screen texture for backdrop blur
uniform sampler2D SCREEN_TEXTURE : hint_screen_texture, filter_linear_mipmap;

group_uniforms PanelDimensions;
// Size of the container in pixels. Updated dynamically on resize.
uniform vec2 container_size = vec2(1232.0, 672.0);
// Corner radius in pixels.
uniform float corner_radius = 24.0;

group_uniforms BlurAndSaturation;
// Blur intensity (Lod level).
uniform float blur_amount : hint_range(0.0, 5.0, 0.1) = 3.0;
// Saturation boost factor for the blurred background.
uniform float saturation_boost : hint_range(1.0, 2.0, 0.05) = 1.25;

group_uniforms FrostedGrain;
// Fine noise grain intensity.
uniform float grain_amount : hint_range(0.0, 0.1, 0.005) = 0.015;

group_uniforms BorderSettings;
// Thickness of the border.
uniform float border_thickness : hint_range(0.5, 5.0, 0.5) = 1.0;
// Color of the top-left highlight border.
uniform vec4 border_color_highlight : source_color = vec4(1.0, 1.0, 1.0, 0.15);
// Color of the bottom-right shadow border.
uniform vec4 border_color_shadow : source_color = vec4(1.0, 1.0, 1.0, 0.06);

group_uniforms ShimmerSheen;
// Intensity of the diagonal shimmer sheen.
uniform float shimmer_amount : hint_range(0.0, 0.2, 0.01) = 0.04;


// Pseudo-random noise function for the frosted grain
float rand(vec2 co) {
	return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453);
}

// Signed Distance Field (SDF) of a rounded box
float sdRoundedBox(vec2 p, vec2 b, float r) {
	vec2 q = abs(p) - b + vec2(r);
	return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

void fragment() {
	// Fallback if size is not yet initialized by the script (prevents division by zero or negative size)
	vec2 size_safe = max(container_size, vec2(2.0));

	// 1. Calculate SDF for the rounded rectangle panel
	vec2 p = UV * size_safe;
	vec2 half_size = size_safe * 0.5;
	vec2 p_centered = p - half_size;
	
	float d = sdRoundedBox(p_centered, half_size, corner_radius);
	
	// Anti-aliased mask for the panel background (disabled if size is uninitialized to avoid clipping)
	float panel_mask = (container_size.x < 1.0) ? 1.0 : smoothstep(0.5, -0.5, d);
	
	// If pixel is outside the rounded rectangle boundary, discard it or set transparent
	if (container_size.x >= 1.0 && d > 0.5) {
		discard;
	}
	
	// 2. Sample screen background with blur
	vec4 blur_sample = textureLod(SCREEN_TEXTURE, SCREEN_UV, blur_amount);
	
	// Apply saturation boost to blurred backdrop colors
	float luma = dot(blur_sample.rgb, vec3(0.299, 0.587, 0.114));
	vec3 saturated_blur = luma + (blur_sample.rgb - luma) * saturation_boost;
	
	// 3. Fallback blending for transparent backgrounds.
	// If nothing is drawn behind the window (blur_sample.a is 0.0),
	// we fall back gracefully to the StyleBox flat tint color (COLOR).
	vec4 tint = COLOR;
	vec3 base_rgb = mix(tint.rgb, saturated_blur, blur_sample.a);
	
	// Blend the frosted color with the panel tint
	vec3 glass_rgb = mix(base_rgb, tint.rgb, tint.a);
	
	// 4. Add fine frosted noise grain (resolution-independent screen-space noise)
	float grain = (rand(FRAGCOORD.xy) - 0.5) * grain_amount;
	glass_rgb = clamp(glass_rgb + vec3(grain), 0.0, 1.0);
	
	// 5. Add diagonal sheen shimmer
	float sheen = clamp((1.0 - UV.x) * (1.0 - UV.y), 0.0, 1.0);
	glass_rgb += sheen * shimmer_amount;
	
	// 6. Procedural rounded border with light source highlights
	// Calculate the normal vector along the border boundary
	vec2 q = abs(p_centered) - half_size + vec2(corner_radius);
	vec2 N = vec2(0.0);
	if (max(q.x, q.y) > 0.0) {
		N = sign(p_centered) * normalize(max(q, 0.0));
	} else {
		N = normalize(p_centered);
	}
	
	// Light source is at the top-left (vector pointing down-right)
	// We check how much the normal aligns with the direction to the light source (-1, -1)
	float light_align = dot(N, normalize(vec2(-1.0, -1.0)));
	float light_intensity = light_align * 0.5 + 0.5; // Map from [-1, 1] to [0, 1]
	
	// Mix highlight and shadow colors based on light alignment
	vec4 border_color = mix(border_color_shadow, border_color_highlight, light_intensity);
	
	// Generate an anti-aliased border mask of thickness border_thickness
	float dist_to_border = abs(d + border_thickness * 0.5);
	float border_mask = smoothstep(border_thickness * 0.5 + 0.5, border_thickness * 0.5 - 0.5, dist_to_border);
	
	// Composite border on top of glass background
	vec3 final_rgb = mix(glass_rgb, border_color.rgb, border_mask * border_color.a);
	
	// Output final pixel with rounded corner anti-aliasing
	COLOR = vec4(final_rgb, tint.a * panel_mask);
}
