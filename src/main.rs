#[macro_use]
extern crate glium;

use cgmath::{Matrix4, PerspectiveFov, Point3, Rad, Vector3};
use glium::{glutin, Surface};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Instant;

const CHUNK_WIDTH: usize = 16;
const CHUNK_HEIGHT: usize = 128;
const CHUNK_DEPTH: usize = 16;
const AIR: u8 = 0;
const STONE: u8 = 1;
const GRASS_BLOCK: u8 = 2;
const DIRT: u8 = 3;
const OAK_LOG: u8 = 4;
const OAK_LEAVES: u8 = 5;
const REDSTONE_BLOCK: u8 = 152;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum WorldStyle {
    Hills,
    Flat,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum GameMode {
    Survival,
    Creative,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum MenuPage {
    Main,
    Modes,
    Settings,
    Pause,
    Playing,
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    color: [f32; 3],
}

implement_vertex!(Vertex, position, tex_coords, color);

#[derive(Copy, Clone)]
struct UiVertex {
    position: [f32; 2],
    color: [f32; 3],
}

implement_vertex!(UiVertex, position, color);

struct Chunk {
    blocks: [u8; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_DEPTH],
}

impl Chunk {
    fn new() -> Self {
        Self {
            blocks: [AIR; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_DEPTH],
        }
    }

    fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_DEPTH + z) * CHUNK_WIDTH + x
    }

    fn set(&mut self, x: usize, y: usize, z: usize, block_id: u8) {
        assert!(x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_DEPTH);
        self.blocks[Self::index(x, y, z)] = block_id;
    }

    fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0
            || y < 0
            || z < 0
            || x >= CHUNK_WIDTH as i32
            || y >= CHUNK_HEIGHT as i32
            || z >= CHUNK_DEPTH as i32
        {
            return AIR;
        }
        self.blocks[Self::index(x as usize, y as usize, z as usize)]
    }

    fn generate(style: WorldStyle, seed: u32) -> Self {
        let mut chunk = Self::new();
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_DEPTH {
                let height = terrain_height(x, z, style, seed);
                for y in 0..height {
                    let block_id = if y == height - 1 {
                        GRASS_BLOCK
                    } else if y + 3 < height {
                        STONE
                    } else {
                        DIRT
                    };
                    chunk.set(x, y, z, block_id);
                }
            }
        }

        if style == WorldStyle::Hills {
            for x in 2..CHUNK_WIDTH - 2 {
                for z in 2..CHUNK_DEPTH - 2 {
                    if coordinate_hash(x, z, seed) % 29 == 0 {
                        let ground = terrain_height(x, z, style, seed);
                        for y in ground..(ground + 4).min(CHUNK_HEIGHT) {
                            chunk.set(x, y, z, OAK_LOG);
                        }
                        let crown = ground + 3;
                        for leaf_x in x.saturating_sub(2)..=(x + 2).min(CHUNK_WIDTH - 1) {
                            for leaf_z in z.saturating_sub(2)..=(z + 2).min(CHUNK_DEPTH - 1) {
                                for leaf_y in
                                    crown.saturating_sub(1)..=(crown + 1).min(CHUNK_HEIGHT - 1)
                                {
                                    if (leaf_x != x || leaf_z != z)
                                        && chunk.get(leaf_x as i32, leaf_y as i32, leaf_z as i32)
                                            == AIR
                                    {
                                        chunk.set(leaf_x, leaf_y, leaf_z, OAK_LEAVES);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        chunk
    }
}

fn coordinate_hash(x: usize, z: usize, seed: u32) -> u32 {
    let mut value =
        seed ^ (x as u32).wrapping_mul(0x9E37_79B9) ^ (z as u32).wrapping_mul(0x85EB_CA6B);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^ (value >> 16)
}

fn terrain_height(x: usize, z: usize, style: WorldStyle, seed: u32) -> usize {
    match style {
        WorldStyle::Flat => 4,
        WorldStyle::Hills => {
            let broad = ((x / 4 + z / 4 + seed as usize) % 3) as isize;
            let detail = (coordinate_hash(x, z, seed) % 3) as isize - 1;
            (5 + broad + detail).clamp(3, 8) as usize
        }
    }
}

#[derive(Copy, Clone)]
enum Face {
    Top,
    Bottom,
    Side,
}

#[derive(Copy, Clone)]
struct Direction {
    offset: [i32; 3],
    face: Face,
    corners: [[f32; 3]; 4],
}

struct MeshBatch {
    block_id: u8,
    texture_file: &'static str,
    vertices: Vec<Vertex>,
}

const DIRECTIONS: [Direction; 6] = [
    Direction {
        offset: [0, 1, 0],
        face: Face::Top,
        corners: [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
    },
    Direction {
        offset: [0, -1, 0],
        face: Face::Bottom,
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    },
    Direction {
        offset: [0, 0, 1],
        face: Face::Side,
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
    },
    Direction {
        offset: [0, 0, -1],
        face: Face::Side,
        corners: [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
    },
    Direction {
        offset: [-1, 0, 0],
        face: Face::Side,
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
    },
    Direction {
        offset: [1, 0, 0],
        face: Face::Side,
        corners: [
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        ],
    },
];

fn texture_for_block(block_id: u8, face: Face) -> &'static str {
    match block_id {
        STONE => "stone.png",
        GRASS_BLOCK => match face {
            Face::Top => "grass_top.png",
            Face::Bottom => "dirt.png",
            Face::Side => "grass_side.png",
        },
        DIRT => "dirt.png",
        OAK_LOG => "oak_log.png",
        OAK_LEAVES => "oak_leaves.png",
        REDSTONE_BLOCK => "redstone_block.png",
        _ => "unknown.png",
    }
}

fn block_fallback_color(block_id: u8) -> [f32; 3] {
    match block_id {
        STONE => [0.52, 0.52, 0.52],
        GRASS_BLOCK => [0.36, 0.63, 0.25],
        DIRT => [0.48, 0.33, 0.20],
        OAK_LOG => [0.38, 0.25, 0.13],
        OAK_LEAVES => [0.19, 0.48, 0.22],
        REDSTONE_BLOCK => redstone_particle_color(),
        _ => [0.8, 0.0, 0.8],
    }
}

fn redstone_particle_color() -> [f32; 3] {
    [0.85, 0.08, 0.12]
}

fn append_face(vertices: &mut Vec<Vertex>, corners: [[f32; 3]; 4], origin: [i32; 3]) {
    let tex_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for index in [0, 1, 2, 0, 2, 3] {
        let corner = corners[index];
        vertices.push(Vertex {
            position: [
                origin[0] as f32 + corner[0],
                origin[1] as f32 + corner[1],
                origin[2] as f32 + corner[2],
            ],
            tex_coords: tex_coords[index],
            color: [1.0, 1.0, 1.0],
        });
    }
}

fn build_chunk_mesh(chunk: &Chunk) -> Vec<MeshBatch> {
    let mut grouped: BTreeMap<(u8, &'static str), Vec<Vertex>> = BTreeMap::new();

    for x in 0..CHUNK_WIDTH {
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_DEPTH {
                let block_id = chunk.get(x as i32, y as i32, z as i32);
                if block_id == AIR {
                    continue;
                }

                for direction in DIRECTIONS {
                    let neighbor = chunk.get(
                        x as i32 + direction.offset[0],
                        y as i32 + direction.offset[1],
                        z as i32 + direction.offset[2],
                    );
                    if neighbor == AIR {
                        let texture_file = texture_for_block(block_id, direction.face);
                        append_face(
                            grouped.entry((block_id, texture_file)).or_default(),
                            direction.corners,
                            [x as i32, y as i32, z as i32],
                        );
                    }
                }
            }
        }
    }

    grouped
        .into_iter()
        .map(|((block_id, texture_file), vertices)| MeshBatch {
            block_id,
            texture_file,
            vertices,
        })
        .collect()
}

fn load_block_texture(
    display: &glium::Display,
    texture_file: &str,
) -> Result<glium::texture::SrgbTexture2d, String> {
    let path = Path::new("assets").join(texture_file);
    let image = image::open(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let rgba = image.to_rgba8();
    let dimensions = rgba.dimensions();
    let raw = glium::texture::RawImage2d::from_raw_rgba_reversed(&rgba.into_raw(), dimensions);
    glium::texture::SrgbTexture2d::new(display, raw)
        .map_err(|error| format!("could not upload {}: {error}", path.display()))
}

fn solid_texture(display: &glium::Display, color: [f32; 3]) -> glium::texture::SrgbTexture2d {
    let rgba = [
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ];
    let raw = glium::texture::RawImage2d::from_raw_rgba(rgba.to_vec(), (1, 1));
    glium::texture::SrgbTexture2d::new(display, raw).expect("create solid block fallback texture")
}

struct MenuButton {
    label: &'static str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn menu_labels(page: MenuPage, world_style: WorldStyle, speed_index: usize) -> Vec<&'static str> {
    match page {
        MenuPage::Main => vec!["PLAY SOLO", "SETTINGS", "QUIT"],
        MenuPage::Modes => vec!["SURVIVAL", "CREATIVE", "BACK"],
        MenuPage::Settings => vec![
            match world_style {
                WorldStyle::Hills => "WORLD HILLS",
                WorldStyle::Flat => "WORLD FLAT",
            },
            match speed_index % 3 {
                0 => "SPEED SLOW",
                1 => "SPEED NORMAL",
                _ => "SPEED FAST",
            },
            "BACK",
        ],
        MenuPage::Pause => vec!["RESUME", "SETTINGS", "MAIN MENU"],
        MenuPage::Playing => Vec::new(),
    }
}

fn menu_layout(width: f32, height: f32, labels: &[&'static str]) -> Vec<MenuButton> {
    let button_width = 252.0_f32.min((width - 32.0).max(0.0));
    let button_height = 38.0_f32.min((height - 24.0).max(0.0));
    let panel_width = button_width + 48.0;
    let button_x = (panel_width - button_width) * 0.5;
    let rows_height =
        labels.len() as f32 * button_height + labels.len().saturating_sub(1) as f32 * 12.0;
    let start_y = (height * 0.5 - rows_height * 0.5)
        .max(112.0)
        .min(height * 0.55);

    labels
        .iter()
        .enumerate()
        .map(|(row, label)| MenuButton {
            label: *label,
            x: button_x,
            y: (start_y + row as f32 * (button_height + 12.0))
                .min((height - button_height - 12.0).max(0.0)),
            width: button_width,
            height: button_height,
        })
        .collect()
}

fn menu_button_at(
    width: f32,
    height: f32,
    point: [f32; 2],
    labels: &[&'static str],
) -> Option<&'static str> {
    menu_layout(width, height, labels)
        .into_iter()
        .find(|button| {
            point[0] >= button.x
                && point[0] <= button.x + button.width
                && point[1] >= button.y
                && point[1] <= button.y + button.height
        })
        .map(|button| button.label)
}

fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        _ => [0; 7],
    }
}

fn push_ui_rect(vertices: &mut Vec<UiVertex>, rect: [f32; 4], color: [f32; 3], screen: [f32; 2]) {
    let [x, y, width, height] = rect;
    let to_ndc = |px: f32, py: f32| [2.0 * px / screen[0] - 1.0, 1.0 - 2.0 * py / screen[1]];
    let corners = [
        to_ndc(x, y),
        to_ndc(x, y + height),
        to_ndc(x + width, y + height),
        to_ndc(x, y),
        to_ndc(x + width, y + height),
        to_ndc(x + width, y),
    ];
    vertices.extend(
        corners
            .into_iter()
            .map(|position| UiVertex { position, color }),
    );
}

fn push_ui_text(
    vertices: &mut Vec<UiVertex>,
    text: &str,
    x: f32,
    y: f32,
    color: [f32; 3],
    screen: [f32; 2],
) {
    push_ui_text_scaled(vertices, text, x, y, color, screen, 1.5);
}

fn push_ui_text_scaled(
    vertices: &mut Vec<UiVertex>,
    text: &str,
    x: f32,
    y: f32,
    color: [f32; 3],
    screen: [f32; 2],
    scale: f32,
) {
    let text_width = (text.chars().count() as f32 * 6.0 - 1.0) * scale;
    let start_x = x - text_width * 0.5;
    let start_y = y - 3.5 * scale;

    for (character_index, character) in text.chars().enumerate() {
        let rows = glyph_rows(character);
        for (row_index, row) in rows.iter().enumerate() {
            for column in 0..5 {
                if row & (1 << (4 - column)) != 0 {
                    push_ui_rect(
                        vertices,
                        [
                            start_x + (character_index as f32 * 6.0 + column as f32) * scale,
                            start_y + row_index as f32 * scale,
                            scale,
                            scale,
                        ],
                        color,
                        screen,
                    );
                }
            }
        }
    }
}

fn build_menu_vertices(
    width: f32,
    height: f32,
    page: MenuPage,
    world_style: WorldStyle,
    speed_index: usize,
    mode: Option<GameMode>,
) -> Vec<UiVertex> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let screen = [width, height];
    let mut vertices = Vec::new();
    if page == MenuPage::Playing {
        push_ui_rect(
            &mut vertices,
            [12.0, 12.0, 260.0, 30.0],
            [0.035, 0.055, 0.075],
            screen,
        );
        push_ui_text(
            &mut vertices,
            "WASD MOVE SPACE UP ESC MENU",
            142.0,
            27.0,
            [0.91, 0.96, 0.93],
            screen,
        );
        return vertices;
    }

    let labels = menu_labels(page, world_style, speed_index);
    let panel_width = menu_layout(width, height, &labels)
        .first()
        .map(|button| button.x * 2.0 + button.width)
        .unwrap_or(0.0);
    push_ui_rect(
        &mut vertices,
        [0.0, 0.0, panel_width + 24.0, height],
        [0.035, 0.055, 0.075],
        screen,
    );
    push_ui_rect(
        &mut vertices,
        [panel_width + 20.0, 0.0, 4.0, height],
        [0.22, 0.68, 0.48],
        screen,
    );
    let title_x = (panel_width + 24.0) * 0.5;
    let title = match page {
        MenuPage::Main => "MAAZ CRAFT",
        MenuPage::Modes => "SELECT MODE",
        MenuPage::Settings => "SETTINGS",
        MenuPage::Pause => "PAUSED",
        MenuPage::Playing => "MAAZ CRAFT",
    };
    push_ui_text_scaled(
        &mut vertices,
        title,
        title_x + 2.0,
        58.0,
        [0.01, 0.02, 0.03],
        screen,
        3.2,
    );
    push_ui_text_scaled(
        &mut vertices,
        title,
        title_x,
        55.0,
        [0.38, 0.86, 0.61],
        screen,
        3.2,
    );
    let subtitle = match (page, mode) {
        (MenuPage::Main, _) => "VOXEL ADVENTURE",
        (MenuPage::Modes, _) => "CHOOSE A WORLD MODE",
        (MenuPage::Settings, _) => "WORLD AND MOVEMENT",
        (MenuPage::Pause, Some(GameMode::Survival)) => "SURVIVAL",
        (MenuPage::Pause, Some(GameMode::Creative)) => "CREATIVE",
        _ => "",
    };
    push_ui_text_scaled(
        &mut vertices,
        subtitle,
        title_x,
        82.0,
        [0.68, 0.76, 0.79],
        screen,
        1.2,
    );

    for button in menu_layout(width, height, &labels) {
        push_ui_rect(
            &mut vertices,
            [button.x, button.y, button.width, button.height],
            [0.10, 0.15, 0.18],
            screen,
        );
        push_ui_rect(
            &mut vertices,
            [button.x, button.y, button.width, 2.0],
            [0.25, 0.72, 0.52],
            screen,
        );
        push_ui_rect(
            &mut vertices,
            [button.x, button.y + button.height - 2.0, button.width, 2.0],
            [0.035, 0.055, 0.075],
            screen,
        );
        push_ui_text(
            &mut vertices,
            button.label,
            button.x + button.width * 0.5,
            button.y + button.height * 0.5,
            [0.91, 0.96, 0.93],
            screen,
        );
    }
    if page == MenuPage::Settings {
        push_ui_text(
            &mut vertices,
            "CLICK TO CHANGE",
            title_x,
            height - 34.0,
            [0.97, 0.79, 0.40],
            screen,
        );
    }
    vertices
}

struct MenuUiBuffer {
    width: u32,
    height: u32,
    page: MenuPage,
    world_style: WorldStyle,
    speed_index: usize,
    mode: Option<GameMode>,
    buffer: glium::VertexBuffer<UiVertex>,
}

fn main() {
    let event_loop = glutin::event_loop::EventLoop::new();
    let window = glutin::window::WindowBuilder::new()
        .with_title("Maaz Craft")
        .with_inner_size(glutin::dpi::LogicalSize::new(854.0, 480.0));
    let context = glutin::ContextBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (2, 1)))
        .with_vsync(true);
    let display =
        glium::Display::new(window, context, &event_loop).expect("create OpenGL 2.1 display");

    let mut world_style = WorldStyle::Hills;
    let mut game_mode = None;
    let chunk = Chunk::generate(world_style, 12);
    let mut mesh_batches = build_chunk_mesh(&chunk);
    let mut vertex_batches: Vec<_> = mesh_batches
        .iter()
        .map(|batch| {
            glium::VertexBuffer::new(&display, &batch.vertices)
                .expect("upload chunk mesh vertex buffer")
        })
        .collect();

    let mut block_textures = BTreeMap::new();
    for batch in &mesh_batches {
        if block_textures.contains_key(batch.texture_file) {
            continue;
        }
        match load_block_texture(&display, batch.texture_file) {
            Ok(texture) => {
                block_textures.insert(batch.texture_file, texture);
            }
            Err(error) => {
                eprintln!("{error}; using a solid block-color fallback.");
            }
        }
    }

    let fallback_textures: BTreeMap<_, _> = [
        STONE,
        GRASS_BLOCK,
        DIRT,
        OAK_LOG,
        OAK_LEAVES,
        REDSTONE_BLOCK,
    ]
    .into_iter()
    .map(|block_id| {
        (
            block_id,
            solid_texture(&display, block_fallback_color(block_id)),
        )
    })
    .collect();

    const VOXEL_VERTEX_SHADER: &str = r#"
        #version 120
        attribute vec3 position;
        attribute vec2 tex_coords;
        attribute vec3 color;
        varying vec2 v_tex_coords;
        varying vec3 v_color;
        uniform mat4 matrix;
        void main() {
            v_tex_coords = tex_coords;
            v_color = color;
            gl_Position = matrix * vec4(position, 1.0);
        }
    "#;

    const VOXEL_FRAGMENT_SHADER: &str = r#"
        #version 120
        varying vec2 v_tex_coords;
        varying vec3 v_color;
        uniform sampler2D block_texture;
        void main() {
            gl_FragColor = texture2D(block_texture, v_tex_coords) * vec4(v_color, 1.0);
        }
    "#;

    const UI_VERTEX_SHADER: &str = r#"
        #version 120
        attribute vec2 position;
        attribute vec3 color;
        varying vec3 v_color;
        void main() {
            v_color = color;
            gl_Position = vec4(position, 0.0, 1.0);
        }
    "#;

    const UI_FRAGMENT_SHADER: &str = r#"
        #version 120
        varying vec3 v_color;
        void main() {
            gl_FragColor = vec4(v_color, 1.0);
        }
    "#;

    let voxel_program =
        glium::Program::from_source(&display, VOXEL_VERTEX_SHADER, VOXEL_FRAGMENT_SHADER, None)
            .expect("compile GLSL 1.20 voxel shaders");
    let ui_program =
        glium::Program::from_source(&display, UI_VERTEX_SHADER, UI_FRAGMENT_SHADER, None)
            .expect("compile GLSL 1.20 menu shaders");
    let indices = glium::index::NoIndices(glium::index::PrimitiveType::TrianglesList);
    let mut menu_buffer: Option<MenuUiBuffer> = None;
    let mut page = MenuPage::Main;
    let mut settings_return_page = MenuPage::Main;
    let mut speed_index = 1usize;
    let mut cursor_position = [0.0f32; 2];
    let mut pressed_keys = HashSet::new();
    let mut camera_position = Vector3::new(23.0, 17.0, 25.0);
    let mut camera_yaw = -2.42f32;
    let mut camera_pitch = -0.48f32;
    let mut previous_frame = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = glutin::event_loop::ControlFlow::Poll;
        match event {
            glutin::event::Event::WindowEvent { event, .. } => match event {
                glutin::event::WindowEvent::CloseRequested => {
                    *control_flow = glutin::event_loop::ControlFlow::Exit;
                }
                glutin::event::WindowEvent::CursorMoved { position, .. } => {
                    cursor_position = [position.x as f32, position.y as f32];
                }
                glutin::event::WindowEvent::MouseInput {
                    state: glutin::event::ElementState::Pressed,
                    button: glutin::event::MouseButton::Left,
                    ..
                } if page != MenuPage::Playing => {
                    let size = display.gl_window().window().inner_size();
                    let labels = menu_labels(page, world_style, speed_index);
                    let clicked = menu_button_at(
                        size.width as f32,
                        size.height as f32,
                        cursor_position,
                        &labels,
                    );
                    match (page, clicked) {
                        (MenuPage::Main, Some("PLAY SOLO")) => page = MenuPage::Modes,
                        (MenuPage::Main, Some("SETTINGS")) => {
                            settings_return_page = MenuPage::Main;
                            page = MenuPage::Settings;
                        }
                        (MenuPage::Main, Some("QUIT")) => {
                            *control_flow = glutin::event_loop::ControlFlow::Exit;
                        }
                        (MenuPage::Modes, Some("SURVIVAL")) => {
                            game_mode = Some(GameMode::Survival);
                            let generated_chunk = Chunk::generate(world_style, 12);
                            mesh_batches = build_chunk_mesh(&generated_chunk);
                            vertex_batches = mesh_batches
                                .iter()
                                .map(|batch| {
                                    glium::VertexBuffer::new(&display, &batch.vertices)
                                        .expect("upload generated world mesh")
                                })
                                .collect();
                            page = MenuPage::Playing;
                        }
                        (MenuPage::Modes, Some("CREATIVE")) => {
                            game_mode = Some(GameMode::Creative);
                            let generated_chunk = Chunk::generate(world_style, 12);
                            mesh_batches = build_chunk_mesh(&generated_chunk);
                            vertex_batches = mesh_batches
                                .iter()
                                .map(|batch| {
                                    glium::VertexBuffer::new(&display, &batch.vertices)
                                        .expect("upload generated world mesh")
                                })
                                .collect();
                            page = MenuPage::Playing;
                        }
                        (MenuPage::Modes, Some("BACK")) => page = MenuPage::Main,
                        (MenuPage::Settings, Some("WORLD HILLS" | "WORLD FLAT")) => {
                            world_style = match world_style {
                                WorldStyle::Hills => WorldStyle::Flat,
                                WorldStyle::Flat => WorldStyle::Hills,
                            };
                        }
                        (
                            MenuPage::Settings,
                            Some("SPEED SLOW" | "SPEED NORMAL" | "SPEED FAST"),
                        ) => {
                            speed_index = (speed_index + 1) % 3;
                        }
                        (MenuPage::Settings, Some("BACK")) => page = settings_return_page,
                        (MenuPage::Pause, Some("RESUME")) => page = MenuPage::Playing,
                        (MenuPage::Pause, Some("SETTINGS")) => {
                            settings_return_page = MenuPage::Pause;
                            page = MenuPage::Settings;
                        }
                        (MenuPage::Pause, Some("MAIN MENU")) => {
                            game_mode = None;
                            page = MenuPage::Main;
                        }
                        _ => {}
                    }
                }
                glutin::event::WindowEvent::KeyboardInput { input, .. } => {
                    if let Some(key) = input.virtual_keycode {
                        match input.state {
                            glutin::event::ElementState::Pressed => {
                                pressed_keys.insert(key);
                                if key == glutin::event::VirtualKeyCode::Escape {
                                    page = match page {
                                        MenuPage::Playing => MenuPage::Pause,
                                        MenuPage::Pause => MenuPage::Playing,
                                        _ => MenuPage::Main,
                                    };
                                }
                            }
                            glutin::event::ElementState::Released => {
                                pressed_keys.remove(&key);
                            }
                        }
                    }
                }
                glutin::event::WindowEvent::Focused(false) => pressed_keys.clear(),
                _ => {}
            },
            glutin::event::Event::MainEventsCleared => {
                let now = Instant::now();
                let delta = now.duration_since(previous_frame).as_secs_f32().min(0.1);
                previous_frame = now;
                if page == MenuPage::Playing {
                    let forward = Vector3::new(camera_yaw.sin(), 0.0, camera_yaw.cos());
                    let right = Vector3::new(camera_yaw.cos(), 0.0, -camera_yaw.sin());
                    let mode_multiplier = if game_mode == Some(GameMode::Creative) {
                        1.6
                    } else {
                        1.0
                    };
                    let speed = [3.0, 5.0, 8.0][speed_index] * mode_multiplier * delta;
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::W) {
                        camera_position += forward * speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::S) {
                        camera_position -= forward * speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::D) {
                        camera_position += right * speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::A) {
                        camera_position -= right * speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Space) {
                        camera_position.y += speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::LShift) {
                        camera_position.y -= speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Left) {
                        camera_yaw -= delta;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Right) {
                        camera_yaw += delta;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Up) {
                        camera_pitch = (camera_pitch + delta).min(1.2);
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Down) {
                        camera_pitch = (camera_pitch - delta).max(-1.2);
                    }
                }
                display.gl_window().window().request_redraw();
            }
            glutin::event::Event::RedrawRequested(_) => {
                let mut target = display.draw();
                target.clear_color_and_depth((0.49, 0.67, 1.0, 1.0), 1.0);

                let (width, height) = target.get_dimensions();
                if height == 0 || width == 0 {
                    return;
                }
                let camera_direction = Vector3::new(
                    camera_yaw.sin() * camera_pitch.cos(),
                    camera_pitch.sin(),
                    camera_yaw.cos() * camera_pitch.cos(),
                );
                let camera_target = camera_position + camera_direction * 12.0;
                let view = Matrix4::look_at_rh(
                    Point3::new(camera_position.x, camera_position.y, camera_position.z),
                    Point3::new(camera_target.x, camera_target.y, camera_target.z),
                    Vector3::unit_y(),
                );
                let projection = Matrix4::from(PerspectiveFov {
                    fovy: Rad(std::f32::consts::FRAC_PI_4),
                    aspect: width as f32 / height as f32,
                    near: 0.1,
                    far: 500.0,
                });
                let matrix: [[f32; 4]; 4] = (projection * view).into();
                let voxel_parameters = glium::DrawParameters {
                    depth: glium::Depth {
                        test: glium::draw_parameters::DepthTest::IfLess,
                        write: true,
                        ..Default::default()
                    },
                    ..Default::default()
                };

                for (batch, vertex_buffer) in mesh_batches.iter().zip(&vertex_batches) {
                    let texture = block_textures
                        .get(batch.texture_file)
                        .unwrap_or(&fallback_textures[&batch.block_id]);
                    target
                        .draw(
                            vertex_buffer,
                            &indices,
                            &voxel_program,
                            &uniform! {
                                matrix: matrix,
                                block_texture: texture,
                            },
                            &voxel_parameters,
                        )
                        .expect("draw chunk mesh");
                }

                let menu_needs_rebuild = menu_buffer
                    .as_ref()
                    .map(|cached| {
                        cached.width != width
                            || cached.height != height
                            || cached.page != page
                            || cached.world_style != world_style
                            || cached.speed_index != speed_index
                            || cached.mode != game_mode
                    })
                    .unwrap_or(true);
                if menu_needs_rebuild {
                    let ui_vertices = build_menu_vertices(
                        width as f32,
                        height as f32,
                        page,
                        world_style,
                        speed_index,
                        game_mode,
                    );
                    let buffer = glium::VertexBuffer::new(&display, &ui_vertices)
                        .expect("upload menu vertex buffer");
                    menu_buffer = Some(MenuUiBuffer {
                        width,
                        height,
                        page,
                        world_style,
                        speed_index,
                        mode: game_mode,
                        buffer,
                    });
                }
                if let Some(cached) = &menu_buffer {
                    let ui_parameters = glium::DrawParameters {
                        depth: glium::Depth {
                            test: glium::draw_parameters::DepthTest::Overwrite,
                            write: false,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    target
                        .draw(
                            &cached.buffer,
                            &indices,
                            &ui_program,
                            &uniform! {},
                            &ui_parameters,
                        )
                        .expect("draw menu overlay");
                }
                target.finish().expect("present frame");
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_stores_classic_block_ids_and_treats_outside_as_air() {
        let mut chunk = Chunk::new();
        for (block_id, x) in [(STONE, 0), (GRASS_BLOCK, 1), (DIRT, 2), (REDSTONE_BLOCK, 3)] {
            chunk.set(x, 0, 0, block_id);
            assert_eq!(chunk.get(x as i32, 0, 0), block_id);
        }
        assert_eq!(chunk.get(-1, 0, 0), AIR);
        assert_eq!(chunk.get(0, CHUNK_HEIGHT as i32, 0), AIR);
    }

    #[test]
    fn mesh_emits_only_air_adjacent_faces() {
        let mut chunk = Chunk::new();
        chunk.set(0, 0, 0, STONE);
        assert_eq!(
            build_chunk_mesh(&chunk)
                .iter()
                .map(|batch| batch.vertices.len())
                .sum::<usize>(),
            36
        );

        chunk.set(1, 0, 0, STONE);
        assert_eq!(
            build_chunk_mesh(&chunk)
                .iter()
                .map(|batch| batch.vertices.len())
                .sum::<usize>(),
            60
        );
    }

    #[test]
    fn block_textures_are_individual_assets() {
        assert_eq!(texture_for_block(STONE, Face::Side), "stone.png");
        assert_eq!(texture_for_block(GRASS_BLOCK, Face::Top), "grass_top.png");
        assert_eq!(texture_for_block(GRASS_BLOCK, Face::Side), "grass_side.png");
        assert_eq!(texture_for_block(DIRT, Face::Side), "dirt.png");
        assert_eq!(
            texture_for_block(REDSTONE_BLOCK, Face::Side),
            "redstone_block.png"
        );
    }

    #[test]
    fn maaz_craft_menu_buttons_fit_and_respond_to_clicks() {
        let labels = menu_labels(MenuPage::Main, WorldStyle::Hills, 1);
        let buttons = menu_layout(854.0, 480.0, &labels);
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[0].label, "PLAY SOLO");
        assert_eq!(buttons[1].label, "SETTINGS");
        assert_eq!(buttons[2].label, "QUIT");
        assert!(buttons.iter().all(|button| button.x >= 0.0));
        assert!(buttons
            .iter()
            .all(|button| button.x + button.width <= 854.0));
        assert_eq!(
            menu_button_at(
                854.0,
                480.0,
                [
                    buttons[0].x + buttons[0].width * 0.5,
                    buttons[0].y + buttons[0].height * 0.5,
                ],
                &labels,
            ),
            Some("PLAY SOLO")
        );
        assert_eq!(menu_button_at(854.0, 480.0, [800.0, 400.0], &labels), None);
    }

    #[test]
    fn world_generation_creates_flat_and_hilly_terrain_with_trees() {
        let flat = Chunk::generate(WorldStyle::Flat, 12);
        let hills = Chunk::generate(WorldStyle::Hills, 12);
        assert_eq!(terrain_height(5, 5, WorldStyle::Flat, 12), 4);
        assert_ne!(
            terrain_height(2, 3, WorldStyle::Hills, 12),
            terrain_height(12, 9, WorldStyle::Hills, 12)
        );
        assert_eq!(flat.get(0, 3, 0), GRASS_BLOCK);
        assert!(hills.blocks.contains(&OAK_LOG));
        assert!(hills.blocks.contains(&OAK_LEAVES));
    }

    #[test]
    fn settings_menu_reflects_current_choices() {
        assert_eq!(
            menu_labels(MenuPage::Settings, WorldStyle::Hills, 0),
            ["WORLD HILLS", "SPEED SLOW", "BACK"]
        );
        assert_eq!(
            menu_labels(MenuPage::Settings, WorldStyle::Flat, 2),
            ["WORLD FLAT", "SPEED FAST", "BACK"]
        );
    }

    #[test]
    fn redstone_has_a_distinct_particle_color() {
        assert_eq!(block_fallback_color(REDSTONE_BLOCK), [0.85, 0.08, 0.12]);
        assert_ne!(redstone_particle_color(), block_fallback_color(STONE));
    }
}
