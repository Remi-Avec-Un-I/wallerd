#version 300 es
precision mediump float;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float transition_secs;
uniform float width;
uniform float height;

const float min_tiles = 1.0;
const float max_tiles = 200.0;

out vec4 frag_color;

void main() {
    vec2 screen_size = vec2(width, height);
    vec2 uv = gl_FragCoord.xy / screen_size;
    float progress = clamp(time / transition_secs, 0.0, 1.0);

    vec2 sample_uv = uv;

    if (progress > 0.0 && progress < 1.0) {
        float transition_curve = abs(progress - 0.5) * 2.0;
        float count = floor(mix(min_tiles, max_tiles, transition_curve));

        vec2 tile_size = screen_size / count;
        vec2 sample_pos = floor(gl_FragCoord.xy / tile_size) * tile_size + tile_size * 0.5;
        sample_uv = sample_pos / screen_size;
    }

    vec4 from_col = texture(t_current, sample_uv);
    vec4 to_col = texture(t_next, sample_uv);
    frag_color = mix(from_col, to_col, progress);
}
