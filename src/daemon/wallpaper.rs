use crate::config::parser::{Config, ConfigFile};
use crate::daemon::commands::{Command, ListCmd, ShaderKind, WallpaperCmd};
use crate::daemon::ipc;
use crate::daemon::ipc_responses::{list_profiles_json, list_shaders_json};
use crate::daemon::renderer::{
    DEFAULT_TRANSITION_SECS, RenderConfig, Renderer, SharedGLResources, resolve_shader_dir,
};
use crate::socket::socket_path;
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::backend::ObjectId;
use smithay_client_toolkit::reexports::client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_surface},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct SurfaceEntry {
    layer_surface: LayerSurface,
    renderer: Option<Renderer>,
    output_id: ObjectId,
    is_animating: bool,
}

pub struct WallpaperState {
    registry_state: RegistryState,
    #[allow(dead_code)]
    compositor_state: CompositorState,
    #[allow(dead_code)]
    layer_shell: LayerShell,
    output_state: OutputState,
    // surfaces must be declared before shared so Renderers drop before SharedGLResources
    // the EGL display that SharedGLResources::drop terminates).
    surfaces: HashMap<ObjectId, SurfaceEntry>,
    shared: SharedGLResources,
    initial_tex_uploaded: bool,
    pending_tex_promotion: bool,
    current_image: PathBuf,
    qh: QueueHandle<WallpaperState>,
    config: ConfigFile,
    profile: Option<String>,
    override_constant_shader: Option<String>,
    override_transition_shader: Option<String>,
    render_config: RenderConfig,
    paused: bool,
    exit: bool,
    view_cancel: Arc<AtomicBool>,
    cmd_sender: calloop::channel::Sender<Command>,
    img_sender: calloop::channel::Sender<image::RgbaImage>,
}

impl WallpaperState {
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

    fn output_matches(&self, output: &wl_output::WlOutput) -> bool {
        let displays = &self.active_config().displays;
        if displays.iter().any(|d| d == "*") {
            return true;
        }
        self.output_state
            .info(output)
            .and_then(|i| i.name)
            .map_or(false, |n| displays.contains(&n))
    }

    fn render_surface(&mut self, sid: ObjectId) {
        let qh = self.qh.clone();
        let Some(entry) = self.surfaces.get_mut(&sid) else {
            return;
        };
        let Some(r) = entry.renderer.as_mut() else {
            return;
        };
        let was_transitioning = r.is_transitioning();
        if r.render(&self.shared, &self.render_config) {
            let s = entry.layer_surface.wl_surface();
            s.frame(&qh, s.clone());
            s.commit();
            entry.is_animating = true;
        } else {
            entry.is_animating = false;
        }
        if was_transitioning && !r.is_transitioning() && self.pending_tex_promotion {
            self.shared.commit_transition();
            self.pending_tex_promotion = false;
        }
    }

    fn do_transition(&mut self, img: image::RgbaImage) {
        self.shared.upload_new_image(img);

        if !self.render_config.uses_transition {
            // Immediate swap: commit before rendering so idle branch shows new texture.
            self.shared.commit_transition();
        } else {
            self.pending_tex_promotion = true;
        }

        let qh = self.qh.clone();
        let sids: Vec<ObjectId> = self.surfaces.keys().cloned().collect();
        for sid in sids {
            let Some(entry) = self.surfaces.get_mut(&sid) else {
                continue;
            };
            let Some(r) = entry.renderer.as_mut() else {
                continue;
            };
            if r.begin_transition(&self.shared, &self.render_config) && !entry.is_animating {
                let s = entry.layer_surface.wl_surface();
                s.frame(&qh, s.clone());
                s.commit();
                entry.is_animating = true;
            }
        }
    }

    fn switch_config(&mut self, name: String) {
        self.view_cancel.store(true, Ordering::Relaxed);
        self.profile = if name == "default" { None } else { Some(name) };
        self.override_constant_shader = None;
        self.override_transition_shader = None;
        self.rebuild_render_config();

        let view = crate::daemon::views::build(self.active_config());
        let initial_path = view.initial();
        self.current_image = initial_path.clone();

        let new_cancel = Arc::new(AtomicBool::new(false));
        self.view_cancel = new_cancel.clone();
        let sender = self.cmd_sender.clone();
        view.run(Box::new(move |p: std::path::PathBuf| {
            if new_cancel.load(Ordering::Relaxed) {
                return false;
            }
            sender.send(Command::Wallpaper(WallpaperCmd::Set(p))).ok();
            true
        }));

        let img_sender = self.img_sender.clone();
        std::thread::spawn(move || {
            if let Some(img) = Renderer::decode_image(&initial_path) {
                img_sender.send(img).ok();
            }
        });
    }

    fn handle_command(&mut self, cmd: Command) {
        match cmd {
            Command::Wallpaper(WallpaperCmd::Set(path)) => {
                if !self.paused {
                    self.current_image = path.clone();
                    let img_sender = self.img_sender.clone();
                    std::thread::spawn(move || {
                        if let Some(img) = Renderer::decode_image(&path) {
                            img_sender.send(img).ok();
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
                let sids: Vec<ObjectId> = self
                    .surfaces
                    .iter()
                    .filter(|(_, e)| !e.is_animating)
                    .map(|(id, _)| id.clone())
                    .collect();
                for sid in sids {
                    self.render_surface(sid);
                }
            }
            Command::SetShader(ShaderKind::Transition, name) => {
                self.override_transition_shader = Some(name);
                self.rebuild_render_config();
            }
            Command::List(_) => {}
            Command::Quit => self.exit = true,
        }
    }
}

impl CompositorHandler for WallpaperState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.render_surface(surface.id());
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for WallpaperState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if !self.output_matches(&output) {
            return;
        }
        let surface = self.compositor_state.create_surface(&self.qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Background,
            Some("wallerd"),
            Some(&output),
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.commit();

        let sid = layer_surface.wl_surface().id();
        self.surfaces.insert(
            sid,
            SurfaceEntry {
                layer_surface,
                renderer: None,
                output_id: output.id(),
                is_animating: false,
            },
        );
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let oid = output.id();
        self.surfaces.retain(|_, e| e.output_id != oid);
    }
}

impl LayerShellHandler for WallpaperState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (w, h) = configure.new_size;
        if w == 0 || h == 0 {
            return;
        }
        let sid = layer.wl_surface().id();

        {
            let Some(entry) = self.surfaces.get_mut(&sid) else {
                return;
            };
            let surface_ptr = entry
                .layer_surface
                .wl_surface()
                .id()
                .as_ptr()
                .cast::<c_void>();
            if entry.renderer.is_none() {
                // Upload the initial image with correct scaling the first time a monitor
                // configures (dimensions are only known at this point).
                if !self.initial_tex_uploaded {
                    if let Some(img) = Renderer::decode_image(&self.current_image) {
                        let scaled = Renderer::scale_image(&img, &self.render_config.scaling, w, h);
                        self.shared.set_initial(scaled);
                    }
                    self.initial_tex_uploaded = true;
                }
                entry.renderer = Some(Renderer::new(
                    &mut self.shared,
                    surface_ptr,
                    w,
                    h,
                    &self.render_config,
                ));
            } else if let Some(r) = &mut entry.renderer {
                r.resize(w, h);
            }
        }

        self.render_surface(sid);
    }
}

impl ProvidesRegistryState for WallpaperState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_compositor!(WallpaperState);
delegate_output!(WallpaperState);
delegate_layer!(WallpaperState);
delegate_registry!(WallpaperState);

pub fn run(config: ConfigFile, profile: Option<String>, name: Option<String>) {
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland display");
    let (globals, event_queue) =
        registry_queue_init(&conn).expect("Failed to init Wayland registry");
    let qh = event_queue.handle();

    let mut event_loop: EventLoop<WallpaperState> =
        EventLoop::try_new().expect("Failed to create event loop");
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .expect("Failed to insert Wayland source into event loop");

    let (cmd_sender, cmd_channel) = calloop::channel::channel();
    loop_handle
        .insert_source(cmd_channel, |event, _, state| {
            if let calloop::channel::Event::Msg(cmd) = event {
                state.handle_command(cmd);
            }
        })
        .expect("Failed to insert IPC channel");

    let (img_sender, img_channel) = calloop::channel::channel::<image::RgbaImage>();
    loop_handle
        .insert_source(img_channel, |event, _, state| {
            if let calloop::channel::Event::Msg(img) = event {
                state.do_transition(img);
            }
        })
        .expect("Failed to insert image channel");

    let sock = socket_path(name.as_deref());
    let listener = ipc::create_socket(Path::new(&sock)).expect("Failed to create IPC socket");
    let ipc_sender = cmd_sender.clone();
    let profiles: Vec<String> = config.additional.keys().cloned().collect();
    let config_for_ipc = config.clone();
    let active_profile_arc: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(profile.clone()));
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
            if ipc_sender.send(cmd).is_err() {
                return "error: daemon stopped".to_string();
            }
            "ok".to_string()
        }
        cmd => {
            if ipc_sender.send(cmd).is_err() {
                return "error: daemon stopped".to_string();
            }
            "ok".to_string()
        }
    });

    let active_config = match &profile {
        Some(name) => config.additional.get(name).unwrap_or(&config.default),
        None => &config.default,
    };
    let render_config = WallpaperState::build_render_config(active_config, None, None);

    let view = crate::daemon::views::build(active_config);
    let current_image = view.initial();
    let view_cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = view_cancel.clone();
    let view_sender = cmd_sender.clone();
    view.run(Box::new(move |path: std::path::PathBuf| {
        if cancel_clone.load(Ordering::Relaxed) {
            return false;
        }
        view_sender
            .send(Command::Wallpaper(WallpaperCmd::Set(path)))
            .ok();
        true
    }));

    let compositor_state =
        CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let layer_shell = match LayerShell::bind(&globals, &qh) {
        Ok(ls) => ls,
        Err(_) => {
            log::error!("Compositor does not support wlr-layer-shell. Wallpaper mode unavailable.");
            std::process::exit(1);
        }
    };
    let output_state = OutputState::new(&globals, &qh);

    let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
    let shared = SharedGLResources::new(display_ptr);

    let mut state = WallpaperState {
        registry_state: RegistryState::new(&globals),
        compositor_state,
        layer_shell,
        output_state,
        surfaces: HashMap::new(),
        shared,
        initial_tex_uploaded: false,
        pending_tex_promotion: false,
        current_image,
        qh,
        config,
        profile,
        override_constant_shader: None,
        override_transition_shader: None,
        render_config,
        paused: false,
        exit: false,
        view_cancel,
        cmd_sender,
        img_sender,
    };

    event_loop
        .run(None, &mut state, |state| {
            if state.exit {
                std::process::exit(0);
            }
        })
        .expect("Event loop error");
}
