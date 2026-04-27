#version 300 es
precision mediump float;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float transition_secs;
uniform float width;
uniform float height;

const float PI            = 3.14159265358979;
const float blur_strength = 0.3;
const int   samples       = 20;
const vec2  blur_center   = vec2(0.5, 0.5);

out vec4 frag_color;

float exponential_ease_in_out(float t) {
    if (t <= 0.0 || t >= 1.0) return t;
    t *= 2.0;
    if (t < 1.0)
        return 0.5 * pow(2.0, 10.0 * (t - 1.0));
    return 0.5 * (-pow(2.0, -10.0 * (t - 1.0)) + 2.0);
}

vec4 cross_fade(vec2 uv, float dissolve) {
    return mix(texture(t_current, uv), texture(t_next, uv), dissolve);
}

void main() {
    vec2  uv       = gl_FragCoord.xy / vec2(width, height);
    float progress = clamp(time / transition_secs, 0.0, 1.0);

    if (progress <= 0.0) { frag_color = texture(t_current, uv); return; }
    if (progress >= 1.0) { frag_color = texture(t_next,    uv); return; }

    float dissolve  = exponential_ease_in_out(progress);
    float blur_mix  = 1.0 - smoothstep(0.85, 1.0, progress);
    float strength  = sin(progress * PI) * blur_strength * blur_mix;

    vec2 to_center = blur_center - uv;

    vec4  blurred = vec4(0.0);
    float total   = 0.0;

    for (int i = 0; i < samples; i++) {
        float percent  = float(i) / float(samples - 1);
        float weight   = percent * (1.0 - percent);
        vec2  sample_uv = uv + to_center * percent * strength;
        blurred += cross_fade(sample_uv, dissolve) * weight;
        total   += weight;
    }

    blurred /= max(total, 0.0001);

    frag_color = mix(blurred, texture(t_next, uv), 1.0 - blur_mix);
}
