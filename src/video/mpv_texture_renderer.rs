use bytemuck;
use eframe::glow::{self, HasContext};

// ---------------------------------------------------------------------------
// Cached uniform locations
// ---------------------------------------------------------------------------

struct UniformLocations {
    pos: glow::UniformLocation,
    size: glow::UniformLocation,
    screen_size: glow::UniformLocation,
    texture: glow::UniformLocation,
}

// ---------------------------------------------------------------------------
// GPU resource handles (dropped together on destroy)
// ---------------------------------------------------------------------------

struct GlHandles {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    vertex_buffer: glow::Buffer,
    uniforms: UniformLocations,
}

// ---------------------------------------------------------------------------
// MpvTextureRenderer
// ---------------------------------------------------------------------------

pub struct MpvTextureRenderer {
    handles: Option<GlHandles>,
}

impl MpvTextureRenderer {
    const VS_SOURCE: &'static str = r#"
        #version 330 core
        layout (location = 0) in vec2 aPos;
        uniform vec2 u_pos;
        uniform vec2 u_size;
        uniform vec2 u_screen_size;
        out vec2 TexCoord;
        void main() {
            vec2 pixel_pos  = u_pos + aPos * u_size;
            float ndc_x     = (pixel_pos.x / u_screen_size.x) * 2.0 - 1.0;
            float ndc_y     = 1.0 - (pixel_pos.y / u_screen_size.y) * 2.0;
            gl_Position     = vec4(ndc_x, ndc_y, 0.0, 1.0);
            TexCoord        = aPos; // Y-flip is handled by NDC transform above
        }
    "#;

    const FS_SOURCE: &'static str = r#"
        #version 330 core
        in vec2 TexCoord;
        out vec4 FragColor;
        uniform sampler2D u_texture;
        void main() {
            FragColor = texture(u_texture, TexCoord);
        }
    "#;

    /// Unit quad: two triangles covering [0,1]×[0,1] as a TRIANGLE_STRIP.
    const VERTICES: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];

    pub fn new(gl: &glow::Context) -> Self {
        let handles = unsafe { Self::build_gl_resources(gl) };
        Self {
            handles: Some(handles),
        }
    }

    /// Compile and link shaders, upload vertex data, cache uniform locations.
    unsafe fn build_gl_resources(gl: &glow::Context) -> GlHandles {
        let program = gl.create_program().expect("create GL program");

        let vs = Self::compile_shader(gl, glow::VERTEX_SHADER, Self::VS_SOURCE);
        let fs = Self::compile_shader(gl, glow::FRAGMENT_SHADER, Self::FS_SOURCE);

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            panic!(
                "GL program link error: {}",
                gl.get_program_info_log(program)
            );
        }

        // Shaders are no longer needed once linked.
        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);

        // Upload static vertex data.
        let vertex_array = gl.create_vertex_array().expect("create VAO");
        gl.bind_vertex_array(Some(vertex_array));

        let vertex_buffer = gl.create_buffer().expect("create VBO");
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck::cast_slice(&Self::VERTICES),
            glow::STATIC_DRAW,
        );

        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
        gl.enable_vertex_attrib_array(0);

        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        // Cache uniform locations once — avoids per-frame string lookups.
        let uniforms = UniformLocations {
            pos: gl.get_uniform_location(program, "u_pos").expect("u_pos"),
            size: gl.get_uniform_location(program, "u_size").expect("u_size"),
            screen_size: gl
                .get_uniform_location(program, "u_screen_size")
                .expect("u_screen_size"),
            texture: gl
                .get_uniform_location(program, "u_texture")
                .expect("u_texture"),
        };

        GlHandles {
            program,
            vertex_array,
            vertex_buffer,
            uniforms,
        }
    }

    unsafe fn compile_shader(gl: &glow::Context, kind: u32, source: &str) -> glow::Shader {
        let shader = gl.create_shader(kind).expect("create shader");
        gl.shader_source(shader, source);
        gl.compile_shader(shader);

        if !gl.get_shader_compile_status(shader) {
            panic!(
                "Shader compile error (type={kind}): {}",
                gl.get_shader_info_log(shader)
            );
        }

        shader
    }

    /// Draw the mpv texture into `rect` using the current GL context.
    pub fn paint(
        &self,
        gl: &glow::Context,
        info: &egui::PaintCallbackInfo,
        rect: egui::Rect,
        mpv_tex_id: u32,
    ) {
        let Some(h) = &self.handles else { return };

        unsafe {
            gl.use_program(Some(h.program));

            let ppp = info.pixels_per_point;
            let vp_w = rect.width() * ppp;
            let vp_h = rect.height() * ppp;
            gl.uniform_2_f32(Some(&h.uniforms.pos), 0.0, 0.0);
            gl.uniform_2_f32(Some(&h.uniforms.size), vp_w, vp_h);
            gl.uniform_2_f32(Some(&h.uniforms.screen_size), vp_w, vp_h);

            gl.active_texture(glow::TEXTURE0);
            let native_tex = std::num::NonZeroU32::new(mpv_tex_id).expect("non-zero texture ID");
            gl.bind_texture(glow::TEXTURE_2D, Some(glow::NativeTexture(native_tex)));
            gl.uniform_1_i32(Some(&h.uniforms.texture), 0);

            gl.bind_vertex_array(Some(h.vertex_array));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);

            // Restore default GL state to avoid corrupting the egui render pass.
            gl.bind_vertex_array(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
        }
    }

    pub fn capture_texture(
        &self,
        gl: &glow::Context,
        mpv_tex_id: u32,
        width: u32,
        height: u32,
    ) -> Option<(Vec<u8>, u32, u32)> {
        if mpv_tex_id == 0 {
            return None;
        }

        unsafe {
            let fbo = gl.create_framebuffer().ok()?;
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));

            let native_tex = std::num::NonZeroU32::new(mpv_tex_id)?;
            gl.framebuffer_texture_2d(
                glow::READ_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(glow::NativeTexture(native_tex)),
                0,
            );

            let w = width as usize;
            let h = height as usize;
            let mut rgba_buf = vec![0u8; w * h * 4];
            gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut rgba_buf)),
            );

            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.delete_framebuffer(fbo);

            let mut rgb = Vec::with_capacity(w * h * 3);
            for y in 0..h {
                let src_row = &rgba_buf[y * w * 4..][..w * 4];
                for px in src_row.chunks_exact(4) {
                    rgb.push(px[0]);
                    rgb.push(px[1]);
                    rgb.push(px[2]);
                }
            }

            Some((rgb, width, height))
        }
    }

    /// Release all GPU resources. Safe to call more than once.
    pub fn destroy(&mut self, gl: &glow::Context) {
        if let Some(h) = self.handles.take() {
            unsafe {
                gl.delete_program(h.program);
                gl.delete_vertex_array(h.vertex_array);
                gl.delete_buffer(h.vertex_buffer);
            }
        }
    }
}
