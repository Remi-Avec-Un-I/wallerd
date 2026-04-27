#version 300 es
precision mediump float;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float transition_secs;
uniform float width;
uniform float height;

const float spacing = 25.0;
const float dot_size = 1.0;
const vec4 dot_color = vec4(0.0, 0.0, 0.0, 1.0);

out vec4 frag_color;

void main() {
    float animation_progress = clamp(time / transition_secs, 0.0, 1.0);

    if (animation_progress <= 0.0) {
        vec2 uv0 = gl_FragCoord.xy / vec2(width, height);
        frag_color = texture(t_current, uv0);
        return;
    }

    vec2 screen_size = vec2(width, height);
    vec2 uv = gl_FragCoord.xy / screen_size;
    vec2 grid_count = floor(screen_size / spacing);

    vec2 norm_pos = uv * screen_size / (grid_count * spacing);
    float delay = (norm_pos.x + norm_pos.y) * 0.5;

    float visible_threshold = delay * 0.1 + 0.01;
    if (animation_progress < visible_threshold) {
        frag_color = texture(t_current, uv);
        return;
    }

    float transition = 0.3;
    float scale = smoothstep(
        delay - transition,
        delay + transition,
        animation_progress * (1.0 + transition)
    );

    if (scale < 0.005) {
        frag_color = texture(t_current, uv);
        return;
    }

    vec2 grid_center = (floor(gl_FragCoord.xy / spacing) + 0.5) * spacing;
    float dist = length(gl_FragCoord.xy - grid_center);
    float dot_radius = dot_size * scale * spacing * 0.8;

    float alpha = 1.0 - smoothstep(
        max(dot_radius - 1.5, 0.0),
        dot_radius + 1.5,
        dist
    );

    if (alpha < 0.01) {
        frag_color = texture(t_current, uv);
        return;
    }

    frag_color = mix(texture(t_current, uv), vec4(dot_color.rgb, 1.0), alpha);
}
