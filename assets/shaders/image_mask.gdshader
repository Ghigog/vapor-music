// image_mask.gdshader
// Clips a TextureRect's own pixel content to a rounded-rect or circle via an
// anti-aliased SDF mask. Set corner_radius >= min(size_px)/2 for a circle.

shader_type canvas_item;

// Size of the rect in pixels. Set from script on creation (art rects are
// fixed-size, never resized after setup, so this is set once).
uniform vec2 size_px = vec2(64.0, 64.0);
// Corner radius in pixels. Values >= half the smaller side round to a circle.
uniform float corner_radius : hint_range(0.0, 512.0) = 9999.0;

float sd_rounded_box(vec2 p, vec2 b, float r) {
	vec2 q = abs(p) - b + vec2(r);
	return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

void fragment() {
	vec2 size_safe = max(size_px, vec2(2.0));
	vec2 half_size = size_safe * 0.5;
	vec2 p = (UV * size_safe) - half_size;

	float r = min(corner_radius, min(half_size.x, half_size.y));
	float d = sd_rounded_box(p, half_size, r);
	float mask = smoothstep(0.5, -0.5, d);

	vec4 tex_color = texture(TEXTURE, UV);
	COLOR = vec4(tex_color.rgb, tex_color.a * mask);
}
