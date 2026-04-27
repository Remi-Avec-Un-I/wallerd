#version 300 es
precision mediump float;

// edited from Nikos Papadopoulos, 4rknova / 2013 in shadertoy
// Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported License.

const float T_TRAN = 2.;
const float T_INTR = 2.;
const float T_PADN = 2.;
const float T_TOTL = ((2. * T_TRAN) + T_INTR + T_PADN);

in vec2 uv;
out vec4 out_color;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float width;
uniform float height;

vec3 transition(vec3 tex0, vec3 tex1, float t) {
    return mix(tex0, tex1, t);
}

void main() {
    vec2 st = uv;

    float t = mod(time, T_TOTL);

    float ts0 = T_TRAN;
    float ts1 = ts0 + T_INTR;
    float ts2 = ts1 + T_TRAN;

    if (t < ts0) t = t / ts0;
    else if (t < ts1) t = 1.;
    else if (t < ts2) t = 1. - ((t - ts1) / (ts2 - ts1));
    else t = 0.;

    vec3 tex0 = texture(t_current, st).xyz;
    vec3 tex1 = texture(t_next, st).xyz;
    out_color = vec4(transition(tex0, tex1, t), 1.0);
}
