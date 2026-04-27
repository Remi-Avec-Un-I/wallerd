#version 300 es
precision mediump float;

in vec2 uv;
out vec4 out_color;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float width;
uniform float height;

const vec2 CURVATURE = vec2(6.0, 4.0);

vec2 curve_remap(vec2 coord) {
    coord = coord * 2.0 - 1.0;
    vec2 offset = abs(coord.yx) / vec2(CURVATURE.x, CURVATURE.y);
    coord = coord + coord * offset * offset;
    return coord * 0.5 + 0.5;
}

vec4 sample_aberration(vec2 coord, float strength) {
    vec2 offset = (coord - 0.5) * strength;
    float r = texture(t_current, coord + offset).r;
    float g = texture(t_current, coord).g;
    float b = texture(t_current, coord - offset).b;
    return vec4(r, g, b, 1.0);
}

float scanlines(vec2 coord) {
    return 0.85 + 0.15 * sin(coord.y * height / 2.0 * 3.14159);
}

float vignette(vec2 coord) {
    vec2 d = (coord - 0.5) * 2.0;
    return 1.0 - dot(d, d) * 0.35;
}

vec3 grade(vec3 col) {
    vec3 shadows = vec3(0.08, 0.0, 0.12);
    vec3 highlights = vec3(1.05, 0.92, 1.08);
    float lum = dot(col, vec3(0.2126, 0.7152, 0.0722));
    return mix(col + shadows * (1.0 - lum), col * highlights, lum);
}

void main() {
    vec2 remapped = curve_remap(uv);

    if (remapped.x < 0.0 || remapped.y < 0.0 || remapped.x > 1.0 || remapped.y > 1.0) {
        out_color = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    float aberration = 0.004 + 0.002 * sin(time * 0.7);
    vec4 col = sample_aberration(remapped, aberration);

    vec3 rgb = grade(col.rgb);
    rgb *= scanlines(remapped);
    rgb *= vignette(remapped);

    float pulse = 0.03 * sin(time * 1.3);
    rgb += vec3(pulse, 0.0, pulse * 0.5);

    out_color = vec4(rgb, 1.0);
}
