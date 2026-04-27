#version 300 es
precision mediump float;

in vec2 uv;
out vec4 out_color;

uniform sampler2D t_current;
uniform sampler2D t_next;
uniform float time;
uniform float width;
uniform float height;

void main() {
    out_color = texture(t_current, uv);
}
