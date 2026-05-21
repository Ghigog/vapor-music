// frosted_glass.gdshader
// Approximates the Vapor frosted-glass aesthetic for canvas-item nodes.
//
// Usage:
//   1. Attach this shader as a ShaderMaterial on a ColorRect or Panel.
//   2. The node must be drawn AFTER the content it should blur (later in scene tree).
//   3. Godot's hint_screen_texture samples everything rendered before this node,
//      which gives the "looking through glass" effect without a SubViewport.
//
// For the full SubViewport-based blur pass (performance budget: max 4 simultaneous),
// see docs/ARCHITECTURE.md §Rendering.

shader_type canvas_item;
render_mode blend_mix;

/// Screen texture — everything rendered before this node.
uniform sampler2D screen_texture : hint_screen_texture, repeat_disable, filter_linear_mipmap;

/// Blur spread in pixels. Range 0–20; default 8.
uniform float blur_radius : hint_range(0.0, 20.0) = 8.0;

/// Glass tint: rgba(18, 18, 28, 0.55) — the Vapor glass base.
uniform vec4 glass_tint : source_color = vec4(0.071, 0.071, 0.110, 0.55);

/// Shimmer strength — top-left diagonal gloss.
uniform float shimmer : hint_range(0.0, 1.0) = 0.06;

void fragment() {
	vec2 px = blur_radius / vec2(textureSize(screen_texture, 0));

	// 9-sample Gaussian approximation.
	vec4 col = vec4(0.0);
	col += texture(screen_texture, SCREEN_UV + vec2(-px.x, -px.y)) * 0.0625;
	col += texture(screen_texture, SCREEN_UV + vec2(  0.0, -px.y)) * 0.1250;
	col += texture(screen_texture, SCREEN_UV + vec2( px.x, -px.y)) * 0.0625;
	col += texture(screen_texture, SCREEN_UV + vec2(-px.x,   0.0)) * 0.1250;
	col += texture(screen_texture, SCREEN_UV                      ) * 0.2500;
	col += texture(screen_texture, SCREEN_UV + vec2( px.x,   0.0)) * 0.1250;
	col += texture(screen_texture, SCREEN_UV + vec2(-px.x,  px.y)) * 0.0625;
	col += texture(screen_texture, SCREEN_UV + vec2(  0.0,  px.y)) * 0.1250;
	col += texture(screen_texture, SCREEN_UV + vec2( px.x,  px.y)) * 0.0625;

	// Blend blurred background with glass tint.
	col.rgb = mix(col.rgb, glass_tint.rgb, glass_tint.a);

	// Diagonal shimmer — brightest at top-left corner, fades toward bottom-right.
	float s = clamp((1.0 - UV.x) * (1.0 - UV.y) * 4.0, 0.0, 1.0);
	col.rgb += s * shimmer;

	COLOR = vec4(col.rgb, 1.0);
}
