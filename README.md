### WHAT'S THAT

A daemon (app that runs in background) that can display wallpapers or windows of images. Those images can automatically change, with shader support both for the image standing still and for transitions between images.

---

### SETUP

#### Install

```sh
yay -S wallerd
```

#### Create a config file

`~/.config/wallerd/config.toml`

```toml
[default]
path = "/path/to/wallpaper.png"
displays = ["eDP-1"]
view = "static"
scaling = "fill"
mode = "wallpaper"
```

#### Run

```sh
wallerd
```

---

### CONFIG REFERENCE

#### Profiles

The config supports multiple named profiles in addition to `[default]`:

```toml
[default]
...

["Lakeside"]
...

["Gaming"]
...
```

Start with a specific profile:

```sh
wallerd -p Lakeside
```

Switch at runtime:

```sh
wallerctl config Lakeside
wallerctl config default
```

---

#### All fields

| Field | Type | Default | Description |
|---|---|---|---|
| `mode` | string | `"wallpaper"` | Display mode. See [Modes](#modes). |
| `displays` | list of strings | `["*"]` | Target displays by name, or `"*"` for all. |
| `path` | string | **required** | Path to an image file or a folder of images. |
| `view` | string | **required** | How images are sourced and updated. See [Views](#views). |
| `scaling` | string | `"fill"` | How the image is scaled. See [Scaling](#scaling). |
| `constant_shader` | string | none | Shader applied every frame on top of the image. See [Shaders](#shaders). |
| `transition_shader` | string | none | Shader used when switching images. See [Shaders](#shaders). |
| `transition_duration` | integer | `2` | Duration of the transition effect in seconds. |
| `interval` | integer | `60` | Seconds between image changes. Only used by the `interval` view. |
| `width` | integer | none | Window width in pixels. Only used in `windowed` mode. |
| `height` | integer | none | Window height in pixels. Only used in `windowed` mode. |

---

#### Modes

Controls how wallerd displays the image.

- `wallpaper` — Renders directly to the desktop as a Wayland wallpaper (wlr-layer-shell). Default.
- `maximised` — Opens a maximized window.
- `windowed` — Opens a floating window. Use `width` and `height` to set its size.

---

#### Views

Controls how images are picked and when they change. `path` must be a folder for `interval` and `time`.

- `static` — Displays a single image forever. `path` is a file.
- `interval` — Cycles through images in `path` at a fixed rate. Set `interval` for the delay in seconds between switches. Images are sorted by filename.
- `time` — Picks an image based on the current time of day. Divides 24 hours evenly among all images in `path`. Useful for having a different wallpaper for morning, afternoon, and night. Name files `0.png`, `1.png`, etc.

---

#### Scaling

Controls how the image fits the display.

- `fill` — Crops the image to fill the entire display. No black bars, but edges may be cut off. Default.
- `fit` — Scales the image to fit entirely within the display. Adds black bars if aspect ratios differ.

---

#### Shaders

Shaders are GLSL programs applied by the GPU. There are two kinds:

- **Constant shader** (`constant_shader`) — Runs every frame on top of the current image. Used for visual effects.
- **Transition shader** (`transition_shader`) — Runs during image switches. Duration is controlled by `transition_duration`.

Available shaders:

| Name | Kind | Description |
|---|---|---|
| `retrowave` | constant | Retro synthwave color effect |
| `vhs` | constant | VHS/CRT tape distortion |
| `highlight` | constant | Subtle shimmer highlight |
| `dot_matrix_diagonal` | constant | Dot matrix pattern |
| `fade` | transition | Simple crossfade |
| `pixel` | transition | Pixelated dissolve |
| `zoom_blur` | transition | Zoom with motion blur |
| `hex_grid` | transition | Hexagonal grid wipe |
| `dot_matrix_diagonal` | transition | Dot matrix diagonal wipe |

Override shaders at runtime (until next restart or profile switch):

```sh
wallerctl shader constant vhs
wallerctl shader transition zoom_blur
```

Custom shaders can be placed in:
- `~/.config/wallerd/shaders/constant/<name>/fragment.glsl`
- `~/.config/wallerd/shaders/transition/<name>/fragment.glsl`

---

### WALLERCTL

Control a running wallerd instance:

```sh
wallerctl wallpaper set /path/to/image.png   # set a specific image
wallerctl wallpaper stop                      # pause cycling
wallerctl wallpaper continue                  # resume cycling
wallerctl config <profile>                    # switch profile
wallerctl shader constant <name>              # change constant shader
wallerctl shader transition <name>            # change transition shader
wallerctl list instances                      # list all running instances
wallerctl list profiles                       # list profiles (JSON)
wallerctl list shaders constant               # list constant shaders (JSON)
wallerctl list shaders transition             # list transition shaders (JSON)
wallerctl kill                                # stop the daemon
```

#### Multiple instances

Start named instances:

```sh
wallerd -n primary
wallerd -n secondary
```

Target a specific instance:

```sh
wallerctl -n primary config Lakeside
```

Target all instances at once:

```sh
wallerctl -n '*' wallpaper stop
wallerctl -n '*' kill
```

---

### FULL EXAMPLE CONFIG

```toml
[default]
mode = "wallpaper"
displays = ["*"]
path = "/home/user/Pictures/wallpapers"
view = "time"
scaling = "fill"
constant_shader = "retrowave"
transition_shader = "fade"
transition_duration = 2

["Gaming"]
mode = "wallpaper"
displays = ["DP-1"]
path = "/home/user/Pictures/gaming.png"
view = "static"
scaling = "fill"

["Slideshow"]
mode = "wallpaper"
displays = ["*"]
path = "/home/user/Pictures/photos"
view = "interval"
interval = 300
scaling = "fit"
transition_shader = "zoom_blur"
transition_duration = 1

["Float"]
mode = "windowed"
width = 800
height = 600
path = "/home/user/Pictures/wallpapers"
view = "interval"
interval = 10
scaling = "fill"
constant_shader = "vhs"
```

---

### WHY

Because I wanted a wallpaper that changed dynamically depending on the time of day.

### HOW

It uses OpenGL to render images and apply GLSL shaders on top of them, running as a Wayland layer-shell surface.
