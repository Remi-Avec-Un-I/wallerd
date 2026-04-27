#version 300 es
precision mediump float;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float transition_secs;
uniform float width;
uniform float height;

const float hex_size        = 0.08;
const float randomness      = 0.05;
const float edge_softness   = 0.01;
const float transition_speed = 0.3;

out vec4 frag_color;

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

vec2 hextile(inout vec2 p) {
    const vec2 s  = vec2(1.0, 1.7320508);
    const vec2 hs = vec2(0.5, 0.8660254);

    vec2 a = mod(p, s) - hs;
    vec2 b = mod(p - hs, s) - hs;

    vec2 c  = dot(a, a) < dot(b, b) ? a : b;
    vec2 id = (c - p + hs) / s;

    p = c;
    return floor(id);
}

float hex(vec2 p, float r) {
    p = abs(p);
    return max(p.x * 0.866025 + p.y * 0.5, p.y) - r;
}

void main() {
    vec2 uv       = gl_FragCoord.xy / vec2(width, height);
    float progress = clamp(time / transition_secs, 0.0, 1.0);

    if (progress <= 0.0) {
        frag_color = texture(t_current, uv);
        return;
    }

    if (progress >= 1.0) {
        frag_color = texture(t_next, uv);
        return;
    }

    vec2 p = uv - 0.5;
    p.x *= (width / height) * 0.9;
    vec2 hp = p / hex_size;
    vec2 id = hextile(hp);

    float delay       = (uv.x + uv.y) * 0.5;
    float hex_progress = clamp(smoothstep(
        delay - transition_speed,
        delay + transition_speed,
        progress * (1.0 + transition_speed)
    ), 0.0, 1.0);

    float r    = hash(id);
    float t    = clamp(hex_progress - r * randomness, 0.0, 1.0);
    float h    = hex(hp, t * 0.9);
    float mask = smoothstep(edge_softness, -edge_softness, h);

    frag_color = vec4(mix(texture(t_current, uv).rgb, texture(t_next, uv).rgb, mask), 1.0);
}
