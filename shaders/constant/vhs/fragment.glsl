#version 300 es
precision mediump float;

uniform sampler2D t_current;
uniform float time;
uniform float width;
uniform float height;

// overlay = true, clip_warp = true are baked in

const float scanlines_opacity = 0.4;
const float scanlines_width = 0.25;
const float grille_opacity = 0.3;
const vec2 resolution = vec2(640.0, 480.0);
const bool pixelate = true;
const bool roll = true;
const float roll_speed = 8.0;
const float roll_size = 15.0;
const float roll_variation = 1.8;
const float distort_intensity = 0.05;
const float noise_opacity = 0.4;
const float noise_speed = 5.0;
const float static_noise_intensity = 0.06;
const float aberration = 0.03;
const float brightness = 1.4;
const bool discolor = true;
const float warp_amount = 1.0;
const float vignette_intensity = 0.4;
const float vignette_opacity = 0.5;

out vec4 frag_color;

vec2 random(vec2 uv) {
    uv = vec2(dot(uv, vec2(127.1, 311.7)),
            dot(uv, vec2(269.5, 183.3)));
    return -1.0 + 2.0 * fract(sin(uv) * 43758.5453123);
}

float noise(vec2 uv) {
    vec2 uv_index = floor(uv);
    vec2 uv_fract = fract(uv);
    vec2 blur = smoothstep(0.0, 1.0, uv_fract);
    return mix(
        mix(dot(random(uv_index + vec2(0.0, 0.0)), uv_fract - vec2(0.0, 0.0)),
            dot(random(uv_index + vec2(1.0, 0.0)), uv_fract - vec2(1.0, 0.0)), blur.x),
        mix(dot(random(uv_index + vec2(0.0, 1.0)), uv_fract - vec2(0.0, 1.0)),
            dot(random(uv_index + vec2(1.0, 1.0)), uv_fract - vec2(1.0, 1.0)), blur.x),
        blur.y) * 0.5 + 0.5;
}

vec2 warp(vec2 uv) {
    vec2 delta = uv - 0.5;
    float delta2 = dot(delta, delta);
    float delta4 = delta2 * delta2;
    return uv + delta * delta4 * warp_amount;
}

float border(vec2 uv) {
    float radius = max(min(abs(min(warp_amount, 0.08) * 2.0), 1.0), 1e-5);
    vec2 abs_uv = abs(uv * 2.0 - 1.0) - vec2(1.0 - radius);
    float dist = length(max(vec2(0.0), abs_uv)) / radius;
    return clamp(1.0 - smoothstep(0.96, 1.0, dist), 0.0, 1.0);
}

float vignette(vec2 uv) {
    uv *= 1.0 - uv;
    return pow(uv.x * uv.y * 15.0, vignette_intensity * vignette_opacity);
}

void main() {
    vec2 raw_uv = gl_FragCoord.xy / vec2(width, height);
    vec2 uv = warp(raw_uv);
    vec2 text_uv = uv;
    vec2 roll_uv = vec2(0.0);
    float t = roll ? time : 0.0;

    if (pixelate) {
        text_uv = ceil(uv * resolution) / resolution;
    }

    float roll_line = 0.0;
    if (roll || noise_opacity > 0.0) {
        roll_line = smoothstep(0.3, 0.9, sin(uv.y * roll_size - t * roll_speed));
        roll_line *= roll_line * smoothstep(0.3, 0.9,
                    sin(uv.y * roll_size * roll_variation - t * roll_speed * roll_variation));
        // raw_uv.x matches Godot's UV.x (pre-warp), as in the original
        roll_uv = vec2(roll_line * distort_intensity * (1.0 - raw_uv.x), 0.0);
    }

    vec4 text;
    if (roll) {
        text.r = texture(t_current, text_uv + roll_uv * 0.8 + vec2(aberration, 0.0) * 0.1).r;
        text.g = texture(t_current, text_uv + roll_uv * 1.2 - vec2(aberration, 0.0) * 0.1).g;
        text.b = texture(t_current, text_uv + roll_uv).b;
    } else {
        text.r = texture(t_current, text_uv + vec2(aberration, 0.0) * 0.1).r;
        text.g = texture(t_current, text_uv - vec2(aberration, 0.0) * 0.1).g;
        text.b = texture(t_current, text_uv).b;
    }
    text.a = 1.0;

    float r = text.r;
    float g = text.g;
    float b = text.b;

    if (grille_opacity > 0.0) {
        float pi = 3.14159265;
        r = mix(r, r * smoothstep(0.85, 0.95, abs(sin(uv.x * resolution.x * pi))), grille_opacity);
        g = mix(g, g * smoothstep(0.85, 0.95, abs(sin(1.05 + uv.x * resolution.x * pi))), grille_opacity);
        b = mix(b, b * smoothstep(0.85, 0.95, abs(sin(2.10 + uv.x * resolution.x * pi))), grille_opacity);
    }

    text.r = clamp(r * brightness, 0.0, 1.0);
    text.g = clamp(g * brightness, 0.0, 1.0);
    text.b = clamp(b * brightness, 0.0, 1.0);

    float scanlines = 0.5;
    if (scanlines_opacity > 0.0) {
        scanlines = smoothstep(scanlines_width, scanlines_width + 0.5,
                abs(sin(uv.y * resolution.y * 3.14159265)));
        text.rgb = mix(text.rgb, text.rgb * vec3(scanlines), scanlines_opacity);
    }

    if (noise_opacity > 0.0) {
        float n = smoothstep(0.4, 0.5,
                noise(uv * vec2(2.0, 200.0) + vec2(10.0, time * noise_speed)));
        roll_line *= n * scanlines * clamp(
                    random(ceil(uv * resolution) / resolution + vec2(time * 0.8, 0.0)).x + 0.8,
                    0.0, 1.0);
        text.rgb = clamp(mix(text.rgb, text.rgb + roll_line, noise_opacity), vec3(0.0), vec3(1.0));
    }

    if (static_noise_intensity > 0.0) {
        text.rgb += clamp(
                random(ceil(uv * resolution) / resolution + fract(time)).x,
                0.0, 1.0) * static_noise_intensity;
    }

    text.rgb *= border(uv);
    text.rgb *= vignette(uv);

    if (discolor) {
        vec3 grey = vec3((text.r + text.g + text.b) / 3.0);
        text.rgb = mix(text.rgb, grey, 0.5);
        float mid = pow(0.5, 2.2);
        text.rgb = (text.rgb - vec3(mid)) * 1.2 + mid;
    }

    frag_color = text;
}
