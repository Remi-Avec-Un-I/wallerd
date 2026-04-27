use crate::daemon::egl_window::{
    WlEglWindow, wl_egl_window_create, wl_egl_window_destroy, wl_egl_window_resize,
};
use glow::HasContext;
use khronos_egl as egl;
use std::ffi::c_void;
use std::path::Path;
use std::time::Instant;

pub const DEFAULT_TRANSITION_SECS: f32 = 2.0;

pub struct RenderConfig {
    pub constant_shader_dir: String,
    pub transition_shader_dir: String,
    pub uses_transition: bool,
    pub uses_animated_constant: bool,
    pub transition_secs: f32,
    pub scaling: String,
}

pub struct SharedGLResources {
    egl_lib: egl::DynamicInstance<egl::EGL1_4>,
    egl_display: egl::Display,
    pub egl_config: egl::Config,
    pbuffer_surface: egl::Surface,
    pub resource_ctx: egl::Context,
    gl: glow::Context,

    pub current_tex: glow::NativeTexture,
    pub next_tex: glow::NativeTexture,
    pub start_time: std::time::Instant,
}

impl SharedGLResources {
    pub fn new(display_ptr: *mut c_void) -> Self {
        let egl_lib = unsafe {
            egl::DynamicInstance::<egl::EGL1_4>::load_required().expect("Failed to load EGL")
        };
        let egl_display =
            unsafe { egl_lib.get_display(display_ptr) }.expect("Failed to get EGL display");
        egl_lib
            .initialize(egl_display)
            .expect("Failed to initialize EGL");

        // Request both WINDOW_BIT and PBUFFER_BIT so the same config works for
        // the pbuffer (resource context) and all per-monitor window surfaces.
        let config_attribs = [
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::SURFACE_TYPE,
            (egl::WINDOW_BIT | egl::PBUFFER_BIT) as i32,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES3_BIT as i32,
            egl::NONE,
        ];
        let egl_config = egl_lib
            .choose_first_config(egl_display, &config_attribs)
            .expect("eglChooseConfig failed")
            .expect("No suitable EGL config (WINDOW+PBUFFER+GLES3)");

        let ctx_attribs = [egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE];
        let resource_ctx = egl_lib
            .create_context(egl_display, egl_config, None, &ctx_attribs)
            .expect("Failed to create resource EGL context");

        let pbuffer_attribs = [egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE];
        let pbuffer_surface = egl_lib
            .create_pbuffer_surface(egl_display, egl_config, &pbuffer_attribs)
            .expect("Failed to create pbuffer surface");

        egl_lib
            .make_current(
                egl_display,
                Some(pbuffer_surface),
                Some(pbuffer_surface),
                Some(resource_ctx),
            )
            .expect("Failed to make resource context current");

        let gl = unsafe {
            glow::Context::from_loader_function(|sym| {
                egl_lib
                    .get_proc_address(sym)
                    .map(|f| f as *const c_void)
                    .unwrap_or(std::ptr::null())
            })
        };

        let dummy_tex = unsafe {
            let tex = gl.create_texture().expect("create placeholder texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                1,
                1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(&[0u8, 0, 0, 255]),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            tex
        };

        Self {
            egl_lib,
            egl_display,
            egl_config,
            pbuffer_surface,
            resource_ctx,
            gl,
            current_tex: dummy_tex,
            next_tex: dummy_tex,
            start_time: std::time::Instant::now(),
        }
    }

    fn make_resource_current(&self) {
        self.egl_lib
            .make_current(
                self.egl_display,
                Some(self.pbuffer_surface),
                Some(self.pbuffer_surface),
                Some(self.resource_ctx),
            )
            .expect("make_current(resource_ctx) failed");
    }

    pub fn set_initial(&mut self, img: image::RgbaImage) {
        self.make_resource_current();
        let new_tex = upload_texture(&self.gl, img).expect("set_initial: upload failed");
        unsafe {
            self.gl.delete_texture(self.current_tex);
        }
        self.current_tex = new_tex;
        self.next_tex = new_tex;
        unsafe {
            self.gl.flush();
        }
    }

    pub fn upload_new_image(&mut self, img: image::RgbaImage) {
        self.make_resource_current();
        let new_tex = upload_texture(&self.gl, img).expect("upload_new_image failed");
        unsafe {
            if self.next_tex != self.current_tex {
                self.gl.delete_texture(self.next_tex);
            }
        }
        self.next_tex = new_tex;
        unsafe {
            self.gl.flush();
        }
    }

    pub fn commit_transition(&mut self) {
        if self.current_tex == self.next_tex {
            return;
        }
        self.make_resource_current();
        unsafe {
            self.gl.delete_texture(self.current_tex);
            self.gl.flush();
        }
        self.current_tex = self.next_tex;
    }
}

impl Drop for SharedGLResources {
    fn drop(&mut self) {
        self.make_resource_current();
        unsafe {
            self.gl.delete_texture(self.current_tex);
            if self.next_tex != self.current_tex {
                self.gl.delete_texture(self.next_tex);
            }
        }
        self.egl_lib
            .destroy_context(self.egl_display, self.resource_ctx)
            .ok();
        self.egl_lib
            .destroy_surface(self.egl_display, self.pbuffer_surface)
            .ok();
        // terminate() is called here — all Renderer drops must have already
        // run (enforced by field declaration order in WallpaperState / App).
        self.egl_lib.terminate(self.egl_display).ok();
    }
}

// ---------------------------------------------------------------------------
// Renderer: one instance per monitor output. Owns its EGL window surface and
// context (sharing textures with SharedGLResources), its own shader programs,
// FBO, and all per-monitor state.
// ---------------------------------------------------------------------------
pub struct Renderer {
    egl_lib: egl::DynamicInstance<egl::EGL1_4>,
    egl_display: egl::Display,
    egl_surface: egl::Surface,
    egl_context: egl::Context,
    wl_egl_win: *mut WlEglWindow,
    gl: glow::Context,

    program_constant: glow::NativeProgram,
    program_transition: glow::NativeProgram,

    loc_time: Option<glow::UniformLocation>,
    loc_w: Option<glow::UniformLocation>,
    loc_h: Option<glow::UniformLocation>,

    fbo: glow::NativeFramebuffer,
    fbo_tex: glow::NativeTexture,
    loc_trans_time: Option<glow::UniformLocation>,
    loc_trans_w: Option<glow::UniformLocation>,
    loc_trans_h: Option<glow::UniformLocation>,
    loc_trans_secs: Option<glow::UniformLocation>,
    transition_start: Option<Instant>,
    constant_shader: String,
    transition_shader: String,
    transition_secs: f32,
    pub width: u32,
    pub height: u32,
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.make_current();
        unsafe {
            self.gl.delete_framebuffer(self.fbo);
            self.gl.delete_texture(self.fbo_tex);
            // current_tex / next_tex belong to SharedGLResources — do not delete here.
            self.gl.delete_program(self.program_constant);
            if self.program_transition != self.program_constant {
                self.gl.delete_program(self.program_transition);
            }
        }
        self.egl_lib
            .destroy_surface(self.egl_display, self.egl_surface)
            .ok();
        self.egl_lib
            .destroy_context(self.egl_display, self.egl_context)
            .ok();
        // egl_lib.terminate() is NOT called here; SharedGLResources owns the display lifecycle.
        unsafe {
            wl_egl_window_destroy(self.wl_egl_win);
        }
    }
}

// ---------------------------------------------------------------------------
// Shader cache helpers (module-level free functions)
// ---------------------------------------------------------------------------

fn xdg_config_shader_base() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".config")
        });
    base.join("wallerd").join("shaders")
}

pub fn resolve_shader_dir(kind: &str, name: &str) -> String {
    let candidates = [
        xdg_config_shader_base().join(kind).join(name),
        std::path::PathBuf::from(format!("/usr/share/wallerd/shaders/{kind}/{name}")),
        std::path::PathBuf::from(format!("shaders/{kind}/{name}")),
    ];
    for path in &candidates {
        if path.join("vertex.glsl").exists() {
            return path.to_string_lossy().into_owned();
        }
    }
    format!("shaders/{kind}/{name}")
}

pub fn list_shader_names(kind: &str) -> Vec<String> {
    let dirs = [
        xdg_config_shader_base().join(kind),
        std::path::PathBuf::from(format!("/usr/share/wallerd/shaders/{kind}")),
        std::path::PathBuf::from(format!("shaders/{kind}")),
    ];
    let mut names = std::collections::HashSet::new();
    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Ok(n) = entry.file_name().into_string() {
                        names.insert(n);
                    }
                }
            }
        }
    }
    let mut v: Vec<String> = names.into_iter().collect();
    v.sort();
    v
}

fn shader_cache_dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".cache")
        });
    base.join("wallerd").join("programs")
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn shader_cache_key(shader_dir: &str) -> Option<u64> {
    let mtime_nanos = |path: &str| -> Option<u64> {
        std::fs::metadata(path)
            .ok()?
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
    };
    let vert_ns = mtime_nanos(&format!("{shader_dir}/vertex.glsl"))?;
    let frag_ns = mtime_nanos(&format!("{shader_dir}/fragment.glsl"))?;
    let mut buf = shader_dir.as_bytes().to_vec();
    buf.extend_from_slice(&vert_ns.to_le_bytes());
    buf.extend_from_slice(&frag_ns.to_le_bytes());
    Some(fnv1a(&buf))
}

fn upload_texture(gl: &glow::Context, mut img: image::RgbaImage) -> Option<glow::NativeTexture> {
    image::imageops::flip_vertical_in_place(&mut img);
    let (w, h) = img.dimensions();
    unsafe {
        let tex = gl.create_texture().ok()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w as i32,
            h as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            Some(&img),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        Some(tex)
    }
}

impl Renderer {
    pub fn new(
        shared: &mut SharedGLResources,
        surface_ptr: *mut c_void,
        width: u32,
        height: u32,
        config: &RenderConfig,
    ) -> Self {
        // Load a fresh EGL handle — same underlying libEGL.so as shared.egl_lib,
        // identical function pointers, but separate Rust ownership.
        let egl_lib = unsafe {
            egl::DynamicInstance::<egl::EGL1_4>::load_required().expect("Failed to load EGL")
        };
        let egl_display = shared.egl_display;

        let wl_egl_win = unsafe { wl_egl_window_create(surface_ptr, width as i32, height as i32) };
        assert!(!wl_egl_win.is_null(), "wl_egl_window_create failed");

        let egl_surface = unsafe {
            egl_lib
                .create_window_surface(
                    egl_display,
                    shared.egl_config,
                    wl_egl_win as egl::NativeWindowType,
                    None,
                )
                .expect("Failed to create EGL window surface")
        };

        let ctx_attribs = [egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE];
        // Share with the resource context so textures are visible in both directions.
        let egl_context = egl_lib
            .create_context(
                egl_display,
                shared.egl_config,
                Some(shared.resource_ctx),
                &ctx_attribs,
            )
            .expect("Failed to create EGL context");

        egl_lib
            .make_current(
                egl_display,
                Some(egl_surface),
                Some(egl_surface),
                Some(egl_context),
            )
            .expect("eglMakeCurrent failed");

        let gl = unsafe {
            glow::Context::from_loader_function(|sym| {
                egl_lib
                    .get_proc_address(sym)
                    .map(|f| f as *const c_void)
                    .unwrap_or(std::ptr::null())
            })
        };
        unsafe {
            gl.viewport(0, 0, width as i32, height as i32);
        }

        let program_constant = Self::load_program_impl(&gl, &config.constant_shader_dir);
        let program_transition = if config.uses_transition {
            Self::load_program_impl(&gl, &config.transition_shader_dir)
        } else {
            program_constant
        };
        let (loc_trans_time, loc_trans_w, loc_trans_h) =
            Self::uniform_locs_impl(&gl, program_transition);
        let loc_trans_secs =
            unsafe { gl.get_uniform_location(program_transition, "transition_secs") };
        unsafe {
            gl.use_program(Some(program_constant));
        }
        let (loc_time, loc_w, loc_h) = Self::uniform_locs_impl(&gl, program_constant);

        let (fbo, fbo_tex) = unsafe {
            let tex = gl.create_texture().expect("create fbo texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            let fb = gl.create_framebuffer().expect("create fbo");
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(tex),
                0,
            );
            debug_assert_eq!(
                gl.check_framebuffer_status(glow::FRAMEBUFFER),
                glow::FRAMEBUFFER_COMPLETE
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            (fb, tex)
        };

        Self {
            egl_lib,
            egl_display,
            egl_surface,
            egl_context,
            wl_egl_win,
            gl,
            program_constant,
            program_transition,
            loc_time,
            loc_w,
            loc_h,
            fbo,
            fbo_tex,
            loc_trans_time,
            loc_trans_w,
            loc_trans_h,
            loc_trans_secs,
            transition_start: None,
            constant_shader: config.constant_shader_dir.clone(),
            transition_shader: config.transition_shader_dir.clone(),
            transition_secs: config.transition_secs,
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.make_current();
        self.width = width;
        self.height = height;
        unsafe {
            wl_egl_window_resize(self.wl_egl_win, width as i32, height as i32, 0, 0);
            self.gl.viewport(0, 0, width as i32, height as i32);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.fbo_tex));
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );
        }
    }

    /// Starts a transition to the image already uploaded into `shared.next_tex`.
    /// Returns `true` if animation frames are needed.
    pub fn begin_transition(&mut self, shared: &SharedGLResources, config: &RenderConfig) -> bool {
        self.make_current();
        if !config.uses_transition {
            self.transition_start = None;
            return self.render(shared, config);
        }
        self.transition_secs = config.transition_secs;

        if self.transition_shader != config.transition_shader_dir {
            self.transition_shader = config.transition_shader_dir.clone();
            let old = self.program_transition;
            self.program_transition = Self::load_program_impl(&self.gl, &self.transition_shader);
            unsafe {
                self.gl.delete_program(old);
            }
            let (lt, lw, lh) = Self::uniform_locs_impl(&self.gl, self.program_transition);
            self.loc_trans_time = lt;
            self.loc_trans_w = lw;
            self.loc_trans_h = lh;
            self.loc_trans_secs = unsafe {
                self.gl
                    .get_uniform_location(self.program_transition, "transition_secs")
            };
            unsafe {
                self.gl.use_program(Some(self.program_constant));
            }
        }
        self.transition_start = Some(Instant::now());
        self.render(shared, config)
    }

    /// Draws one frame. Returns `true` if more frames are needed.
    pub fn render(&mut self, shared: &SharedGLResources, config: &RenderConfig) -> bool {
        self.make_current();
        let raw_elapsed = self.transition_start.map(|s| s.elapsed().as_secs_f32());

        if raw_elapsed.is_none() && self.constant_shader != config.constant_shader_dir {
            self.constant_shader = config.constant_shader_dir.clone();
            let old = self.program_constant;
            self.program_constant = Self::load_program_impl(&self.gl, &self.constant_shader);
            unsafe {
                self.gl.delete_program(old);
            }
            let (loc_time, loc_w, loc_h) = Self::uniform_locs_impl(&self.gl, self.program_constant);
            self.loc_time = loc_time;
            self.loc_w = loc_w;
            self.loc_h = loc_h;
        }

        let global_time = shared.start_time.elapsed().as_secs_f32();

        unsafe {
            if let Some(elapsed) = raw_elapsed {
                let transition_time = elapsed.min(self.transition_secs);

                // Pass 1: transition shader → FBO
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
                self.gl.use_program(Some(self.program_transition));
                self.gl
                    .uniform_1_f32(self.loc_trans_time.as_ref(), transition_time);
                self.gl
                    .uniform_1_f32(self.loc_trans_w.as_ref(), self.width as f32);
                self.gl
                    .uniform_1_f32(self.loc_trans_h.as_ref(), self.height as f32);
                self.gl
                    .uniform_1_f32(self.loc_trans_secs.as_ref(), self.transition_secs);
                self.gl.active_texture(glow::TEXTURE0);
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(shared.current_tex));
                self.gl.active_texture(glow::TEXTURE1);
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(shared.next_tex));
                self.gl.draw_arrays(glow::TRIANGLES, 0, 6);

                // Pass 2: constant shader reads fbo_tex, writes to screen
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                self.gl.use_program(Some(self.program_constant));
                self.gl.uniform_1_f32(self.loc_time.as_ref(), global_time);
                self.gl
                    .uniform_1_f32(self.loc_w.as_ref(), self.width as f32);
                self.gl
                    .uniform_1_f32(self.loc_h.as_ref(), self.height as f32);
                self.gl.active_texture(glow::TEXTURE0);
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.fbo_tex));
                self.gl.draw_arrays(glow::TRIANGLES, 0, 6);
            } else {
                // Idle: single pass, constant shader directly to screen
                self.gl.use_program(Some(self.program_constant));
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                self.gl.uniform_1_f32(self.loc_time.as_ref(), global_time);
                self.gl
                    .uniform_1_f32(self.loc_w.as_ref(), self.width as f32);
                self.gl
                    .uniform_1_f32(self.loc_h.as_ref(), self.height as f32);
                self.gl.active_texture(glow::TEXTURE0);
                self.gl
                    .bind_texture(glow::TEXTURE_2D, Some(shared.current_tex));
                if shared.current_tex != shared.next_tex {
                    self.gl.active_texture(glow::TEXTURE1);
                    self.gl
                        .bind_texture(glow::TEXTURE_2D, Some(shared.next_tex));
                }
                self.gl.draw_arrays(glow::TRIANGLES, 0, 6);
            }
        }

        let still_transitioning = raw_elapsed.map_or(false, |e| e < self.transition_secs);
        let transition_finished = raw_elapsed.map_or(false, |e| e >= self.transition_secs);

        self.egl_lib
            .swap_buffers(self.egl_display, self.egl_surface)
            .ok();

        if transition_finished {
            // Textures are owned by SharedGLResources; commit_transition() is called
            // by the caller (WallpaperState/App) after this returns false.
            if self.constant_shader != config.constant_shader_dir {
                self.constant_shader = config.constant_shader_dir.clone();
                let old = self.program_constant;
                self.program_constant = Self::load_program_impl(&self.gl, &self.constant_shader);
                unsafe {
                    self.gl.delete_program(old);
                }
                let (loc_time, loc_w, loc_h) =
                    Self::uniform_locs_impl(&self.gl, self.program_constant);
                self.loc_time = loc_time;
                self.loc_w = loc_w;
                self.loc_h = loc_h;
            }
            self.transition_start = None;
        }

        still_transitioning || config.uses_animated_constant
    }

    pub fn is_transitioning(&self) -> bool {
        self.transition_start.is_some()
    }

    pub fn decode_image(path: &Path) -> Option<image::RgbaImage> {
        match image::open(path) {
            Ok(img) => Some(img.to_rgba8()),
            Err(e) => {
                log::error!("Failed to open {:?}: {e}", path);
                None
            }
        }
    }

    pub fn scale_image(
        img: &image::RgbaImage,
        scaling: &str,
        disp_w: u32,
        disp_h: u32,
    ) -> image::RgbaImage {
        if disp_w == 0 || disp_h == 0 {
            return img.clone();
        }
        let (img_w, img_h) = img.dimensions();
        if img_w == 0 || img_h == 0 {
            return img.clone();
        }
        let img_ar = img_w as f64 / img_h as f64;
        let disp_ar = disp_w as f64 / disp_h as f64;
        match scaling {
            "fill" => {
                let (crop_w, crop_h, x, y) = if disp_ar > img_ar {
                    let h = (img_w as f64 / disp_ar).round() as u32;
                    let y = (img_h.saturating_sub(h)) / 2;
                    (img_w, h.min(img_h), 0, y)
                } else {
                    let w = (img_h as f64 * disp_ar).round() as u32;
                    let x = (img_w.saturating_sub(w)) / 2;
                    (w.min(img_w), img_h, x, 0)
                };
                image::imageops::crop_imm(img, x, y, crop_w, crop_h).to_image()
            }
            "fit" => {
                let (canvas_w, canvas_h, x, y) = if disp_ar > img_ar {
                    let w = (img_h as f64 * disp_ar).round() as u32;
                    let x = (w.saturating_sub(img_w)) / 2;
                    (w, img_h, x, 0)
                } else {
                    let h = (img_w as f64 / disp_ar).round() as u32;
                    let y = (h.saturating_sub(img_h)) / 2;
                    (img_w, h, 0, y)
                };
                let mut canvas =
                    image::RgbaImage::from_pixel(canvas_w, canvas_h, image::Rgba([0, 0, 0, 255]));
                image::imageops::overlay(&mut canvas, img, x as i64, y as i64);
                canvas
            }
            _ => img.clone(),
        }
    }

    fn make_current(&self) {
        self.egl_lib
            .make_current(
                self.egl_display,
                Some(self.egl_surface),
                Some(self.egl_surface),
                Some(self.egl_context),
            )
            .expect("make_current(monitor) failed");
    }

    fn init_program_samplers(gl: &glow::Context, prog: glow::NativeProgram) {
        unsafe {
            gl.use_program(Some(prog));
            if let Some(loc) = gl.get_uniform_location(prog, "t_current") {
                gl.uniform_1_i32(Some(&loc), 0);
            }
            if let Some(loc) = gl.get_uniform_location(prog, "t_next") {
                gl.uniform_1_i32(Some(&loc), 1);
            }
        }
    }

    fn compile_program_from_source(gl: &glow::Context, shader_dir: &str) -> glow::NativeProgram {
        let vert_src = std::fs::read_to_string(format!("{shader_dir}/vertex.glsl"))
            .unwrap_or_else(|e| panic!("Cannot read {shader_dir}/vertex.glsl: {e}"));
        let frag_src = std::fs::read_to_string(format!("{shader_dir}/fragment.glsl"))
            .unwrap_or_else(|e| panic!("Cannot read {shader_dir}/fragment.glsl: {e}"));
        unsafe {
            let vert = gl
                .create_shader(glow::VERTEX_SHADER)
                .expect("create vertex shader");
            gl.shader_source(vert, &vert_src);
            gl.compile_shader(vert);
            if !gl.get_shader_compile_status(vert) {
                panic!(
                    "Vertex shader error ({shader_dir}): {}",
                    gl.get_shader_info_log(vert)
                );
            }
            let frag = gl
                .create_shader(glow::FRAGMENT_SHADER)
                .expect("create fragment shader");
            gl.shader_source(frag, &frag_src);
            gl.compile_shader(frag);
            if !gl.get_shader_compile_status(frag) {
                panic!(
                    "Fragment shader error ({shader_dir}): {}",
                    gl.get_shader_info_log(frag)
                );
            }
            let prog = gl.create_program().expect("create program");
            gl.program_binary_retrievable_hint(prog, true);
            gl.attach_shader(prog, vert);
            gl.attach_shader(prog, frag);
            gl.link_program(prog);
            if !gl.get_program_link_status(prog) {
                panic!(
                    "Shader link error ({shader_dir}): {}",
                    gl.get_program_info_log(prog)
                );
            }
            gl.detach_shader(prog, vert);
            gl.detach_shader(prog, frag);
            gl.delete_shader(vert);
            gl.delete_shader(frag);
            prog
        }
    }

    fn try_load_cached_program(
        gl: &glow::Context,
        cache_path: &std::path::Path,
    ) -> Option<glow::NativeProgram> {
        let data = std::fs::read(cache_path).ok()?;
        if data.len() < 4 {
            let _ = std::fs::remove_file(cache_path);
            return None;
        }
        let pb = glow::ProgramBinary {
            format: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            buffer: data[4..].to_vec(),
        };
        unsafe {
            let prog = gl.create_program().ok()?;
            gl.program_binary(prog, &pb);
            if gl.get_program_link_status(prog) {
                Some(prog)
            } else {
                gl.delete_program(prog);
                let _ = std::fs::remove_file(cache_path);
                None
            }
        }
    }

    fn save_program_to_cache(
        gl: &glow::Context,
        prog: glow::NativeProgram,
        cache_path: &std::path::Path,
    ) {
        unsafe {
            let pb = match gl.get_program_binary(prog) {
                Some(pb) if !pb.buffer.is_empty() => pb,
                _ => return,
            };
            if let Some(parent) = cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut data = pb.format.to_le_bytes().to_vec();
            data.extend_from_slice(&pb.buffer);
            let _ = std::fs::write(cache_path, &data);
        }
    }

    fn load_program_impl(gl: &glow::Context, shader_dir: &str) -> glow::NativeProgram {
        let cache_path = shader_cache_key(shader_dir)
            .map(|key| shader_cache_dir().join(format!("{key:016x}.bin")));

        if let Some(ref path) = cache_path {
            if let Some(prog) = Self::try_load_cached_program(gl, path) {
                log::debug!("shader cache hit: {shader_dir}");
                Self::init_program_samplers(gl, prog);
                return prog;
            }
        }

        log::debug!("shader cache miss, compiling: {shader_dir}");
        let prog = Self::compile_program_from_source(gl, shader_dir);

        if let Some(ref path) = cache_path {
            Self::save_program_to_cache(gl, prog, path);
        }

        Self::init_program_samplers(gl, prog);
        prog
    }

    fn uniform_locs_impl(
        gl: &glow::Context,
        prog: glow::NativeProgram,
    ) -> (
        Option<glow::UniformLocation>,
        Option<glow::UniformLocation>,
        Option<glow::UniformLocation>,
    ) {
        unsafe {
            (
                gl.get_uniform_location(prog, "time"),
                gl.get_uniform_location(prog, "width"),
                gl.get_uniform_location(prog, "height"),
            )
        }
    }
}
