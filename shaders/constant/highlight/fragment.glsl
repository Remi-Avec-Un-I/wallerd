#version 300 es
precision mediump float;

uniform sampler2D t_current;
uniform float time;
uniform float width;
uniform float height;

const float Line_Smoothness = 0.045;
const float Line_Width = 0.03;
const float Brightness = 2.0;
const float Rotation_deg = 30.0;
const float Distortion = 1.8;
const float Speed = 0.2;
const float Position = 0.0;
const float Position_Min = 0.25;
const float Position_Max = 0.5;
const float Alpha = 1.0;

out vec4 frag_color;

// blend_premul_alpha baked into output: mix(wallpaper, white, highlight)

vec2 rotate_uv(vec2 uv, vec2 center, float rotation, bool use_degrees) {
    float angle = use_degrees ? rotation * (3.1415926 / 180.0) : rotation;
    mat2 rot = mat2(
            vec2(cos(angle), -sin(angle)),
            vec2(sin(angle), cos(angle))
        );
    return rot * (uv - center) + center;
}

void main() {
    vec2 uv = gl_FragCoord.xy / vec2(width, height);

    vec2 center_uv = uv - vec2(0.5);
    float gradient_to_edge = 1.0 - max(abs(center_uv.x), abs(center_uv.y)) * Distortion;

    vec2 rotated_uv = rotate_uv(uv, vec2(0.5), Rotation_deg, true);

    float remapped_position = Position_Min + (Position_Max - Position_Min) * Position;
    float remapped_time = fract(time * Speed + remapped_position);
    remapped_time = -2.0 + 4.0 * remapped_time; // [0,1] → [-2,2]

    float line = sqrt(gradient_to_edge * abs(rotated_uv.x + remapped_time));

    float line_smoothness = clamp(Line_Smoothness, 0.001, 1.0);
    float offset_plus = Line_Width + line_smoothness;
    float offset_minus = Line_Width - line_smoothness;

    float remapped_line = (line - offset_plus) / (offset_minus - offset_plus);
    remapped_line = clamp(min(remapped_line * Brightness, Alpha), 0.0, 1.0);

    vec4 base = texture(t_current, uv);
    frag_color = vec4(mix(base.rgb, vec3(1.0), remapped_line), 1.0);
}
