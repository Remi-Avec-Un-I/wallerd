use crate::config::parser::{Config, ConfigFile};
use crate::daemon::commands::{Command, ListCmd, ShaderKind, WallpaperCmd};
use crate::daemon::ipc;
use crate::daemon::ipc_responses::{list_profiles_json, list_shaders_json};
use crate::daemon::renderer::{
    DEFAULT_TRANSITION_SECS, RenderConfig, Renderer, SharedGLResources, resolve_shader_dir,
};
use crate::socket::socket_path;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::{Fullscreen, Window, WindowId};

pub enum AppEvent {
    Command(Command),
    ImageReady(image::RgbaImage),
}

pub struct App {
    config: ConfigFile,
    profile: Option<String>,
    name: Option<String>,
    override_constant_shader: Option<String>,
    override_transition_shader: Option<String>,
    // Drop order matters: renderer → shared (eglTerminate) → window (Wayland surface).
    // Rust drops fields in declaration order, so window must come last.
    renderer: Option<Renderer>,
    shared: Option<SharedGLResources>,
    window: Option<Arc<Window>>,
    pending_tex_promotion: bool,
    proxy: EventLoopProxy<AppEvent>,
    paused: bool,
    view_cancel: Option<Arc<AtomicBool>>,
    render_config: RenderConfig,
}

impl App {
    pub fn new(
        config: ConfigFile,
        profile: Option<String>,
        name: Option<String>,
        proxy: EventLoopProxy<AppEvent>,
    ) -> Self {
        let active = match &profile {
            Some(name) => config.additional.get(name).unwrap_or(&config.default),
            None => &config.default,
        };
        let render_config = Self::build_render_config(active, None, None);
        Self {
            config,
            profile,
            name,
            override_constant_shader: None,
            override_transition_shader: None,
            window: None,
            renderer: None,
            shared: None,
            pending_tex_promotion: false,
            proxy,
            paused: false,
            view_cancel: None,
            render_config,
        }
    }

    fn active_config(&self) -> &Config {
        match &self.profile {
            Some(name) => self.config.additional.get(name).unwrap_or_else(|| {
                log::warn!("Profile '{name}' not found, using default.");
                &self.config.default
            }),
            None => &self.config.default,
        }
    }

    fn build_render_config(
        active: &Config,
        override_constant: Option<&str>,
        override_transition: Option<&str>,
    ) -> RenderConfig {
        let constant_name = override_constant
            .or(active.constant_shader.as_deref())
            .unwrap_or("default");
        let transition_name = override_transition
            .or(active.transition_shader.as_deref())
            .unwrap_or("default");
        RenderConfig {
            constant_shader_dir: resolve_shader_dir("constant", constant_name),
            transition_shader_dir: resolve_shader_dir("transition", transition_name),
            uses_transition: transition_name != "default",
            uses_animated_constant: constant_name != "default",
            transition_secs: active
                .transition_duration
                .map(|d| d as f32)
                .unwrap_or(DEFAULT_TRANSITION_SECS),
            scaling: active.scaling.clone(),
        }
    }

    fn rebuild_render_config(&mut self) {
        self.render_config = Self::build_render_config(
            self.active_config(),
            self.override_constant_shader.as_deref(),
            self.override_transition_shader.as_deref(),
        );
    }

    fn do_transition(&mut self, img: image::RgbaImage) {
        if let Some(shared) = self.shared.as_mut() {
            shared.upload_new_image(img);
        } else {
            return;
        }
        if !self.render_config.uses_transition {
            if let Some(shared) = self.shared.as_mut() {
                shared.commit_transition();
            }
        } else {
            self.pending_tex_promotion = true;
        }
        let shared = match self.shared.as_ref() {
            Some(s) => s,
            None => return,
        };
        let needs_redraw = if let Some(r) = &mut self.renderer {
            r.begin_transition(shared, &self.render_config)
        } else {
            return;
        };
        if needs_redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn switch_config(&mut self, name: String) {
        if let Some(cancel) = &self.view_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.profile = if name == "default" { None } else { Some(name) };
        self.override_constant_shader = None;
        self.override_transition_shader = None;
        self.rebuild_render_config();

        let view = crate::daemon::views::build(self.active_config());
        let initial_path = view.initial();

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_view = cancel.clone();
        self.view_cancel = Some(cancel);
        let proxy = self.proxy.clone();
        view.run(Box::new(move |p: PathBuf| {
            if cancel_for_view.load(Ordering::Relaxed) {
                return false;
            }
            proxy
                .send_event(AppEvent::Command(Command::Wallpaper(WallpaperCmd::Set(p))))
                .ok();
            true
        }));

        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            if let Some(img) = Renderer::decode_image(&initial_path) {
                proxy.send_event(AppEvent::ImageReady(img)).ok();
            }
        });
    }

    fn render(&mut self) {
        let shared = match self.shared.as_ref() {
            Some(s) => s,
            None => return,
        };
        let was_transitioning = self
            .renderer
            .as_ref()
            .map_or(false, |r| r.is_transitioning());
        let needs_redraw = if let Some(r) = &mut self.renderer {
            r.render(shared, &self.render_config)
        } else {
            return;
        };
        let transition_just_finished = was_transitioning
            && !self
                .renderer
                .as_ref()
                .map_or(false, |r| r.is_transitioning());
        if transition_just_finished && self.pending_tex_promotion {
            if let Some(shared) = self.shared.as_mut() {
                shared.commit_transition();
            }
            self.pending_tex_promotion = false;
        }
        if needs_redraw {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }
}

pub fn create_event_loop(
    config: ConfigFile,
    profile: Option<String>,
    profiles: Vec<String>,
    name: Option<&str>,
) -> (EventLoop<AppEvent>, EventLoopProxy<AppEvent>) {
    let event_loop = EventLoop::<AppEvent>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let sock = socket_path(name);
    let listener = ipc::create_socket(Path::new(&sock)).unwrap();
    let ipc_proxy = proxy.clone();
    let config_for_ipc = config;
    let active_profile_arc: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(profile));
    let active_ipc = active_profile_arc.clone();
    ipc::handle_stream(listener, move |cmd| match cmd {
        Command::List(ListCmd::Profiles) => {
            let active = active_ipc.lock().unwrap().clone();
            list_profiles_json(&config_for_ipc, active.as_deref())
        }
        Command::List(ListCmd::Shaders(ref kind)) => list_shaders_json(kind),
        Command::Config(ref name) => {
            if name != "default" && !profiles.contains(name) {
                return format!("error: unknown profile '{name}'");
            }
            *active_ipc.lock().unwrap() = if name == "default" {
                None
            } else {
                Some(name.clone())
            };
            if ipc_proxy.send_event(AppEvent::Command(cmd)).is_err() {
                return "error: daemon stopped".to_string();
            }
            "ok".to_string()
        }
        Command::Quit => {
            let _ = ipc_proxy.send_event(AppEvent::Command(Command::Quit));
            "ok".to_string()
        }
        cmd => {
            if ipc_proxy.send_event(AppEvent::Command(cmd)).is_err() {
                return "error: daemon stopped".to_string();
            }
            "ok".to_string()
        }
    });
    (event_loop, proxy)
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mode = self.active_config().mode.clone();
        let width = self.active_config().width;
        let height = self.active_config().height;
        let window_name = self
            .name
            .as_deref()
            .map(|n| format!("windowed-wallerd-{n}"))
            .unwrap_or_else(|| "windowed-wallerd".to_string());
        let mut attrs = Window::default_attributes()
            .with_title(&window_name)
            .with_name(&window_name, &window_name);
        attrs = match mode.as_str() {
            "wallpaper" => attrs
                .with_decorations(false)
                .with_fullscreen(Some(Fullscreen::Borderless(None))),
            "maximised" => attrs.with_maximized(true),
            "windowed" | _ => {
                if let (Some(w), Some(h)) = (width, height) {
                    attrs.with_inner_size(LogicalSize::new(w, h))
                } else {
                    attrs
                }
            }
        };
        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        let display_ptr = match window.display_handle().unwrap().as_raw() {
            RawDisplayHandle::Wayland(h) => h.display.as_ptr() as *mut c_void,
            _ => panic!("wallerd requires a Wayland display"),
        };
        let surface_ptr = match window.window_handle().unwrap().as_raw() {
            RawWindowHandle::Wayland(h) => h.surface.as_ptr() as *mut c_void,
            _ => panic!("wallerd requires a Wayland window"),
        };

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let view = crate::daemon::views::build(self.active_config());
        let initial_path = view.initial();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_view = cancel.clone();
        self.view_cancel = Some(cancel);
        let proxy = self.proxy.clone();
        view.run(Box::new(move |path: PathBuf| {
            if cancel_for_view.load(Ordering::Relaxed) {
                return false;
            }
            proxy
                .send_event(AppEvent::Command(Command::Wallpaper(WallpaperCmd::Set(
                    path,
                ))))
                .ok();
            true
        }));

        let mut shared = SharedGLResources::new(display_ptr);
        if let Some(img) = Renderer::decode_image(&initial_path) {
            let scaled = Renderer::scale_image(&img, &self.render_config.scaling, width, height);
            shared.set_initial(scaled);
        }
        self.renderer = Some(Renderer::new(
            &mut shared,
            surface_ptr,
            width,
            height,
            &self.render_config,
        ));
        self.shared = Some(shared);
        self.window = Some(window);

        self.render();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::ImageReady(img) => {
                if !self.paused {
                    self.do_transition(img);
                }
            }
            AppEvent::Command(cmd) => match cmd {
                Command::Wallpaper(WallpaperCmd::Set(path)) => {
                    if !self.paused {
                        let proxy = self.proxy.clone();
                        std::thread::spawn(move || {
                            if let Some(img) = Renderer::decode_image(&path) {
                                proxy.send_event(AppEvent::ImageReady(img)).ok();
                            }
                        });
                    }
                }
                Command::Wallpaper(WallpaperCmd::Stop) => self.paused = true,
                Command::Wallpaper(WallpaperCmd::Continue) => self.paused = false,
                Command::Config(name) => self.switch_config(name),
                Command::SetShader(ShaderKind::Constant, name) => {
                    self.override_constant_shader = Some(name);
                    self.rebuild_render_config();
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                Command::SetShader(ShaderKind::Transition, name) => {
                    self.override_transition_shader = Some(name);
                    self.rebuild_render_config();
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                Command::List(_) => {}
                Command::Quit => event_loop.exit(),
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let width = size.width.max(1);
                let height = size.height.max(1);
                if let Some(r) = &mut self.renderer {
                    r.resize(width, height);
                }
                self.render();
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}
