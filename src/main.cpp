#include <GLES3/gl3.h>
#include <emscripten.h>
#include <emscripten/html5.h>
#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <vector>

EM_JS(void, set_status, (const char* message), {
    document.getElementById('status').textContent = UTF8ToString(message);
});

namespace {
GLuint program = 0;
GLuint vao = 0;
GLint scale_location = -1;

[[noreturn]] void fail(const char* message) {
    std::fprintf(stderr, "%s\n", message);
    set_status(message);
    std::exit(EXIT_FAILURE);
}

GLuint compile(GLenum type, const char* source) {
    GLuint shader = glCreateShader(type);
    glShaderSource(shader, 1, &source, nullptr);
    glCompileShader(shader);
    GLint ok = 0;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        GLint size = 0;
        glGetShaderiv(shader, GL_INFO_LOG_LENGTH, &size);
        std::vector<char> log(std::max(size, 1));
        glGetShaderInfoLog(shader, size, nullptr, log.data());
        fail(log.data());
    }
    return shader;
}

void frame() {
    double css_width = 0, css_height = 0;
    emscripten_get_element_css_size("#canvas", &css_width, &css_height);
    const double dpr = emscripten_get_device_pixel_ratio();
    const int width = std::max(1, static_cast<int>(css_width * dpr));
    const int height = std::max(1, static_cast<int>(css_height * dpr));
    int old_width = 0, old_height = 0;
    emscripten_get_canvas_element_size("#canvas", &old_width, &old_height);
    if (old_width != width || old_height != height)
        emscripten_set_canvas_element_size("#canvas", width, height);
    glViewport(0, 0, width, height);
    glClearColor(0.035f, 0.055f, 0.09f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    glUseProgram(program);
    const float shortest = static_cast<float>(std::min(width, height));
    glUniform2f(scale_location, shortest / width, shortest / height);
    glBindVertexArray(vao);
    glDrawElements(GL_TRIANGLES, 6, GL_UNSIGNED_SHORT, nullptr);
}
}

int main() {
    EmscriptenWebGLContextAttributes attributes;
    emscripten_webgl_init_context_attributes(&attributes);
    attributes.majorVersion = 2;
    attributes.alpha = EM_FALSE;
    attributes.depth = EM_FALSE;
    attributes.stencil = EM_FALSE;
    attributes.antialias = EM_TRUE;
    const auto context = emscripten_webgl_create_context("#canvas", &attributes);
    if (context <= 0) fail("WebGL 2 недоступен в этом браузере.");
    if (emscripten_webgl_make_context_current(context) != EMSCRIPTEN_RESULT_SUCCESS)
        fail("Не удалось активировать WebGL 2.");

    const char* vertex_source = R"(#version 300 es
        layout(location = 0) in vec2 position;
        layout(location = 1) in vec3 color;
        uniform vec2 scale;
        out vec3 vertex_color;
        void main() {
            gl_Position = vec4(position * scale, 0.0, 1.0);
            vertex_color = color;
        }
    )";
    const char* fragment_source = R"(#version 300 es
        precision mediump float;
        in vec3 vertex_color;
        out vec4 fragment_color;
        void main() { fragment_color = vec4(vertex_color, 1.0); }
    )";
    GLuint vertex = compile(GL_VERTEX_SHADER, vertex_source);
    GLuint fragment = compile(GL_FRAGMENT_SHADER, fragment_source);
    program = glCreateProgram();
    glAttachShader(program, vertex);
    glAttachShader(program, fragment);
    glLinkProgram(program);
    glDeleteShader(vertex);
    glDeleteShader(fragment);
    GLint linked = 0;
    glGetProgramiv(program, GL_LINK_STATUS, &linked);
    if (!linked) {
        char log[2048] = {};
        glGetProgramInfoLog(program, sizeof(log), nullptr, log);
        fail(log);
    }
    scale_location = glGetUniformLocation(program, "scale");
    // Four vertices (XY + RGB), two indexed triangles.
    constexpr float vertices[] = {
        -0.6f, -0.6f, 0.20f, 0.40f, 1.00f,
         0.6f, -0.6f, 0.65f, 0.25f, 0.95f,
         0.6f,  0.6f, 0.10f, 0.85f, 0.90f,
        -0.6f,  0.6f, 0.20f, 0.95f, 0.60f,
    };
    constexpr GLushort indices[] = {0, 1, 2, 2, 3, 0};
    GLuint vbo = 0, ebo = 0;
    glGenVertexArrays(1, &vao);
    glBindVertexArray(vao);
    glGenBuffers(1, &vbo);
    glBindBuffer(GL_ARRAY_BUFFER, vbo);
    glBufferData(GL_ARRAY_BUFFER, sizeof(vertices), vertices, GL_STATIC_DRAW);
    glGenBuffers(1, &ebo);
    glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, ebo);
    glBufferData(GL_ELEMENT_ARRAY_BUFFER, sizeof(indices), indices, GL_STATIC_DRAW);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, 5 * sizeof(float), nullptr);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(1, 3, GL_FLOAT, GL_FALSE, 5 * sizeof(float),
                          reinterpret_cast<const void*>(2 * sizeof(float)));
    glEnableVertexAttribArray(1);
    set_status("WebGL 2 · WASM · 4 вершины · 2 треугольника");
    emscripten_set_main_loop(frame, 0, true);
}
