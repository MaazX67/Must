#[macro_use]
extern crate glium;

use cgmath::{InnerSpace, Matrix4, PerspectiveFov, Point3, Rad, Vector3};
use glium::{glutin, Surface};
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const CHUNK_WIDTH: usize = 16;
const CHUNK_HEIGHT: usize = 128;
const CHUNK_DEPTH: usize = 16;
const WORLD_BLOCK_COUNT: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_DEPTH;
const MAX_SAVED_WORLDS: usize = 64;
const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_EYE_HEIGHT: f32 = 1.62;
const AIR: u8 = 0;
const STONE: u8 = 1;
const GRASS_BLOCK: u8 = 2;
const DIRT: u8 = 3;
const OAK_LOG: u8 = 4;
const OAK_LEAVES: u8 = 5;
const OAK_PLANKS: u8 = 6;
const COBBLESTONE: u8 = 7;
const GLASS: u8 = 8;
const PATH_BLOCK: u8 = 9;
const SNOW_BLOCK: u8 = 10;
const ICE: u8 = 11;
const PACKED_ICE: u8 = 12;
const BLUE_ICE: u8 = 13;
const SPRUCE_LOG: u8 = 14;
const SPRUCE_LEAVES: u8 = 15;
const ICE_BRICKS: u8 = 16;
const CRYSTAL: u8 = 17;
const JUNGLE_LOG: u8 = 18;
const JUNGLE_LEAVES: u8 = 19;
const RED_SAND: u8 = 20;
const RED_SANDSTONE: u8 = 21;
const MAGENTA_CRYSTAL: u8 = 22;
const REDSTONE_BLOCK: u8 = 152;

#[derive(Clone)]
struct WorldSave {
    name: String,
    style: WorldStyle,
    seed: u32,
    berries: u8,
    blocks: Vec<u8>,
}

fn white_texture(display: &glium::Display) -> glium::texture::SrgbTexture2d {
    let raw = glium::texture::RawImage2d::from_raw_rgba(vec![255u8; 4], (1, 1));
    glium::texture::SrgbTexture2d::new(display, raw).expect("create mob tint texture")
}

impl WorldSave {
    fn new(name: String, style: WorldStyle, seed: u32) -> Self {
        let chunk = Chunk::generate(style, seed);
        Self {
            name,
            style,
            seed,
            berries: 6,
            blocks: chunk.blocks.to_vec(),
        }
    }

    fn chunk(&self) -> Result<Chunk, String> {
        Chunk::from_blocks(&self.blocks)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum WorldStyle {
    Hills,
    Flat,
    Frostlands,
    Frontier,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Biome {
    Jungle,
    RedDesert,
    FrostTundra,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum MobKind {
    JungleStalker,
    DuneRaider,
    FrostWisp,
    HillMite,
}

#[derive(Copy, Clone, Debug)]
struct Mob {
    kind: MobKind,
    position: Vector3<f32>,
    health: u8,
    attack_timer: f32,
    alive: bool,
}

const MAX_MOBS: usize = 3;
const MOB_VERTEX_COUNT: usize = 144;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum GameMode {
    Survival,
    Creative,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum MenuPage {
    Main,
    Worlds,
    Modes,
    Settings,
    Pause,
    Playing,
    Dead,
}

#[derive(Clone, Debug)]
struct SurvivalState {
    health: u8,
    hunger: u8,
    berries: u8,
    hunger_timer: f32,
    healing_timer: f32,
    starvation_timer: f32,
    airborne: bool,
    fall_start_y: f32,
}

impl SurvivalState {
    fn new(berries: u8) -> Self {
        Self {
            health: 20,
            hunger: 20,
            berries,
            hunger_timer: 0.0,
            healing_timer: 0.0,
            starvation_timer: 0.0,
            airborne: false,
            fall_start_y: 0.0,
        }
    }

    fn tick(&mut self, elapsed: f32, moving: bool) {
        if self.health == 0 {
            return;
        }
        self.hunger_timer += elapsed;
        let hunger_period = if moving { 90.0 } else { 180.0 };
        while self.hunger_timer >= hunger_period {
            self.hunger_timer -= hunger_period;
            self.hunger = self.hunger.saturating_sub(1);
        }

        if self.hunger >= 18 && self.health < 20 {
            self.healing_timer += elapsed;
            while self.healing_timer >= 5.0 && self.health < 20 && self.hunger >= 18 {
                self.healing_timer -= 5.0;
                self.health += 1;
                self.hunger -= 1;
            }
        } else {
            self.healing_timer = 0.0;
        }

        if self.hunger == 0 {
            self.starvation_timer += elapsed;
            while self.starvation_timer >= 6.0 && self.health > 0 {
                self.starvation_timer -= 6.0;
                self.health = self.health.saturating_sub(1);
            }
        } else {
            self.starvation_timer = 0.0;
        }
    }

    fn consume_berry(&mut self) -> bool {
        if self.berries == 0 || self.health == 0 || self.hunger == 20 {
            return false;
        }
        self.berries -= 1;
        self.hunger = (self.hunger + 5).min(20);
        self.starvation_timer = 0.0;
        true
    }

    fn harvest_berry(&mut self) -> bool {
        if self.berries >= 32 {
            return false;
        }
        self.berries += 1;
        true
    }

    fn damage(&mut self, amount: u8) {
        self.health = self.health.saturating_sub(amount);
    }

    fn update_fall(&mut self, camera_y: f32, grounded: bool) -> u8 {
        if !grounded {
            if !self.airborne {
                self.fall_start_y = camera_y;
                self.airborne = true;
            }
            return 0;
        }
        if !self.airborne {
            return 0;
        }
        self.airborne = false;
        let distance = self.fall_start_y - camera_y;
        if distance > 3.0 {
            ((distance - 3.0).floor() as u8).saturating_mul(2)
        } else {
            0
        }
    }
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
    alpha: f32,
}

implement_vertex!(UiVertex, position, color, alpha);

struct Chunk {
    blocks: [u8; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_DEPTH],
}

impl Chunk {
    fn new() -> Self {
        Self {
            blocks: [AIR; WORLD_BLOCK_COUNT],
        }
    }

    fn from_blocks(blocks: &[u8]) -> Result<Self, String> {
        let block_array: [u8; WORLD_BLOCK_COUNT] = blocks.try_into().map_err(|_| {
            format!(
                "world has {} blocks; expected {WORLD_BLOCK_COUNT}",
                blocks.len()
            )
        })?;
        Ok(Self {
            blocks: block_array,
        })
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

    fn set_if_in_bounds(&mut self, x: i32, y: i32, z: i32, block_id: u8) -> bool {
        if x < 0
            || y < 0
            || z < 0
            || x >= CHUNK_WIDTH as i32
            || y >= CHUNK_HEIGHT as i32
            || z >= CHUNK_DEPTH as i32
        {
            return false;
        }
        self.set(x as usize, y as usize, z as usize, block_id);
        true
    }

    fn generate(style: WorldStyle, seed: u32) -> Self {
        let mut chunk = Self::new();
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_DEPTH {
                let height = terrain_height(x, z, style, seed);
                for y in 0..height {
                    let block_id = terrain_block(x, y, z, height, style, seed);
                    chunk.set(x, y, z, block_id);
                }
            }
        }

        if style == WorldStyle::Hills {
            for x in 2..CHUNK_WIDTH - 2 {
                for z in 2..CHUNK_DEPTH - 2 {
                    if coordinate_hash(x, z, seed) % 29 == 0
                        && !((2..=13).contains(&x) && (2..=9).contains(&z))
                    {
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
        match style {
            WorldStyle::Frostlands => {
                for (x, z) in [(2, 4), (2, 11), (14, 5), (14, 12)] {
                    build_snow_pine(&mut chunk, x, z, seed);
                }
                build_ice_temple(&mut chunk);
            }
            WorldStyle::Frontier => build_frontier(&mut chunk, seed),
            WorldStyle::Hills | WorldStyle::Flat => build_village(&mut chunk),
        }
        chunk
    }
}

fn world_store_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let data_root = env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "APPDATA is not set; cannot locate world storage".to_string())?;

    #[cfg(not(target_os = "windows"))]
    let data_root = if let Some(path) = env::var_os("XDG_DATA_HOME") {
        PathBuf::from(path)
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is not set; cannot locate world storage".to_string())?
            .join(".local")
            .join("share")
    };

    Ok(data_root.join("MaazCraft").join("worlds.dat"))
}

fn encode_worlds(worlds: &[WorldSave]) -> Result<String, String> {
    if worlds.len() > MAX_SAVED_WORLDS {
        return Err(format!(
            "cannot save {} worlds; limit is {MAX_SAVED_WORLDS}",
            worlds.len()
        ));
    }

    let mut output = String::from("MAAZCRAFT_WORLDS_2\n");
    for world in worlds {
        if world.name.is_empty()
            || world.name.contains(['|', '\n', '\r'])
            || world.blocks.len() != WORLD_BLOCK_COUNT
            || world.berries > 32
        {
            return Err("world name, berry count, or block data is invalid".to_string());
        }
        let style = match world.style {
            WorldStyle::Hills => "hills",
            WorldStyle::Flat => "flat",
            WorldStyle::Frostlands => "frostlands",
            WorldStyle::Frontier => "frontier",
        };
        output.push_str(&format!(
            "{}|{}|{}|{}|",
            world.name, style, world.seed, world.berries
        ));
        for byte in &world.blocks {
            output.push_str(&format!("{byte:02x}"));
        }
        output.push('\n');
    }
    Ok(output)
}

fn decode_worlds(encoded: &str) -> Result<Vec<WorldSave>, String> {
    let mut lines = encoded.lines();
    let format_version = match lines.next() {
        Some("MAAZCRAFT_WORLDS_1") => 1,
        Some("MAAZCRAFT_WORLDS_2") => 2,
        _ => return Err("unsupported or invalid world-save header".to_string()),
    };

    let mut worlds = Vec::new();
    for (line_index, line) in lines.enumerate() {
        if worlds.len() >= MAX_SAVED_WORLDS {
            return Err(format!("world-save limit of {MAX_SAVED_WORLDS} exceeded"));
        }
        let mut fields = line.splitn(if format_version == 1 { 4 } else { 5 }, '|');
        let name = fields.next().unwrap_or_default();
        let style = match fields.next() {
            Some("hills") => WorldStyle::Hills,
            Some("flat") => WorldStyle::Flat,
            Some("frostlands") => WorldStyle::Frostlands,
            Some("frontier") => WorldStyle::Frontier,
            _ => return Err(format!("invalid style in world entry {}", line_index + 1)),
        };
        let seed = fields
            .next()
            .ok_or_else(|| format!("missing seed in world entry {}", line_index + 1))?
            .parse::<u32>()
            .map_err(|error| format!("invalid seed in world entry {}: {error}", line_index + 1))?;
        let berries = if format_version == 1 {
            6
        } else {
            fields
                .next()
                .ok_or_else(|| format!("missing berry count in world entry {}", line_index + 1))?
                .parse::<u8>()
                .map_err(|error| {
                    format!(
                        "invalid berry count in world entry {}: {error}",
                        line_index + 1
                    )
                })?
        };
        if berries > 32 {
            return Err(format!(
                "berry count exceeds limit in world entry {}",
                line_index + 1
            ));
        }
        let hex_blocks = fields
            .next()
            .ok_or_else(|| format!("missing blocks in world entry {}", line_index + 1))?;
        if name.is_empty() || hex_blocks.len() != WORLD_BLOCK_COUNT * 2 {
            return Err(format!("invalid data in world entry {}", line_index + 1));
        }
        let mut blocks = Vec::with_capacity(WORLD_BLOCK_COUNT);
        for pair in hex_blocks.as_bytes().chunks_exact(2) {
            let pair = std::str::from_utf8(pair)
                .map_err(|error| format!("invalid block data in world entry: {error}"))?;
            blocks.push(
                u8::from_str_radix(pair, 16)
                    .map_err(|error| format!("invalid block data in world entry: {error}"))?,
            );
        }
        worlds.push(WorldSave {
            name: name.to_string(),
            style,
            seed,
            berries,
            blocks,
        });
    }
    Ok(worlds)
}

fn load_worlds(path: &Path) -> Result<Vec<WorldSave>, String> {
    let backup_path = path.with_extension("dat.bak");
    match fs::read_to_string(path) {
        Ok(contents) => match decode_worlds(&contents) {
            Ok(worlds) => Ok(worlds),
            Err(primary_error) => match load_backup_worlds(&backup_path)? {
                Some(worlds) => Ok(worlds),
                None => Err(format!(
                    "invalid world save {}: {primary_error}",
                    path.display()
                )),
            },
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(load_backup_worlds(&backup_path)?.unwrap_or_default())
        }
        Err(primary_error) => match load_backup_worlds(&backup_path)? {
            Some(worlds) => Ok(worlds),
            None => Err(format!(
                "could not read world save {}: {primary_error}",
                path.display()
            )),
        },
    }
}

fn load_backup_worlds(path: &Path) -> Result<Option<Vec<WorldSave>>, String> {
    match fs::read_to_string(path) {
        Ok(contents) => decode_worlds(&contents)
            .map(Some)
            .map_err(|error| format!("invalid world backup {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "could not read world backup {}: {error}",
            path.display()
        )),
    }
}

fn save_worlds(path: &Path, worlds: &[WorldSave]) -> Result<(), String> {
    let contents = encode_worlds(worlds)?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("world-save path {} has no parent", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let temporary_path = path.with_extension("dat.tmp");
    fs::write(&temporary_path, contents)
        .map_err(|error| format!("could not write {}: {error}", temporary_path.display()))?;
    let backup_path = path.with_extension("dat.bak");
    if path.exists() {
        if backup_path.exists() {
            fs::remove_file(&backup_path)
                .map_err(|error| format!("could not remove {}: {error}", backup_path.display()))?;
        }
        fs::rename(path, &backup_path)
            .map_err(|error| format!("could not back up {}: {error}", path.display()))?;
    }
    if let Err(error) = fs::rename(&temporary_path, path) {
        if backup_path.exists() {
            if let Err(restore_error) = fs::rename(&backup_path, path) {
                return Err(format!(
                    "could not replace {} ({error}) or restore backup ({restore_error})",
                    path.display()
                ));
            }
        }
        return Err(format!(
            "could not replace world save {}: {error}",
            path.display()
        ));
    }
    if backup_path.exists() {
        fs::remove_file(&backup_path)
            .map_err(|error| format!("could not remove {}: {error}", backup_path.display()))?;
    }
    Ok(())
}

fn persist_worlds(
    path: Option<&Path>,
    storage_write_enabled: bool,
    worlds: &[WorldSave],
) -> Result<(), String> {
    if !storage_write_enabled {
        return Err("world storage is read-only after a load or save error".to_string());
    }
    let path = path.ok_or_else(|| "world-save location is unavailable".to_string())?;
    save_worlds(path, worlds)
}

fn upload_chunk_mesh(
    display: &glium::Display,
    chunk: &Chunk,
) -> (Vec<MeshBatch>, Vec<glium::VertexBuffer<Vertex>>) {
    let batches = build_chunk_mesh(chunk);
    let buffers = batches
        .iter()
        .map(|batch| {
            glium::VertexBuffer::new(display, &batch.vertices).expect("upload generated world mesh")
        })
        .collect();
    (batches, buffers)
}

fn load_mesh_textures(
    display: &glium::Display,
    batches: &[MeshBatch],
    textures: &mut BTreeMap<&'static str, glium::texture::SrgbTexture2d>,
) {
    for batch in batches {
        if textures.contains_key(batch.texture_file) {
            continue;
        }
        match load_block_texture(display, batch.texture_file) {
            Ok(texture) => {
                textures.insert(batch.texture_file, texture);
            }
            Err(error) => {
                eprintln!("{error}; using a procedural pixel-texture fallback.");
            }
        }
    }
}

fn spawn_mobs(style: WorldStyle, seed: u32, chunk: &Chunk) -> Vec<Mob> {
    let candidates = [(2usize, 11usize), (7, 2), (13, 10)];
    candidates
        .into_iter()
        .take(MAX_MOBS)
        .enumerate()
        .map(|(index, (x, z))| {
            let kind = match style {
                WorldStyle::Frontier => match frontier_biome(x) {
                    Biome::Jungle => MobKind::JungleStalker,
                    Biome::RedDesert => MobKind::DuneRaider,
                    Biome::FrostTundra => MobKind::FrostWisp,
                },
                WorldStyle::Frostlands => MobKind::FrostWisp,
                WorldStyle::Flat | WorldStyle::Hills => MobKind::HillMite,
            };
            let offset = (coordinate_hash(x, z, seed ^ index as u32) % 3) as f32 * 0.1;
            Mob {
                kind,
                position: Vector3::new(
                    x as f32 + 0.5,
                    surface_height(chunk, x, z) as f32 + offset,
                    z as f32 + 0.5,
                ),
                health: 8,
                attack_timer: 0.0,
                alive: true,
            }
        })
        .collect()
}

fn mob_color(kind: MobKind) -> [f32; 3] {
    match kind {
        MobKind::JungleStalker => [0.12, 0.58, 0.24],
        MobKind::DuneRaider => [0.78, 0.31, 0.16],
        MobKind::FrostWisp => [0.30, 0.72, 0.94],
        MobKind::HillMite => [0.60, 0.23, 0.62],
    }
}

fn append_tinted_face(
    vertices: &mut Vec<Vertex>,
    corners: [[f32; 3]; 4],
    origin: [f32; 3],
    color: [f32; 3],
) {
    let tex_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for index in [0, 1, 2, 0, 2, 3] {
        let corner = corners[index];
        vertices.push(Vertex {
            position: [
                origin[0] + corner[0],
                origin[1] + corner[1],
                origin[2] + corner[2],
            ],
            tex_coords: tex_coords[index],
            color,
        });
    }
}

fn append_mob_box(vertices: &mut Vec<Vertex>, min: [f32; 3], max: [f32; 3], tint: [f32; 3]) {
    let faces = [
        [
            [min[0], max[1], min[2]],
            [min[0], max[1], max[2]],
            [max[0], max[1], max[2]],
            [max[0], max[1], min[2]],
        ],
        [
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [max[0], min[1], min[2]],
            [min[0], min[1], min[2]],
        ],
        [
            [min[0], min[1], max[2]],
            [max[0], min[1], max[2]],
            [max[0], max[1], max[2]],
            [min[0], max[1], max[2]],
        ],
        [
            [max[0], min[1], min[2]],
            [min[0], min[1], min[2]],
            [min[0], max[1], min[2]],
            [max[0], max[1], min[2]],
        ],
        [
            [min[0], min[1], min[2]],
            [min[0], min[1], max[2]],
            [min[0], max[1], max[2]],
            [min[0], max[1], min[2]],
        ],
        [
            [max[0], min[1], max[2]],
            [max[0], min[1], min[2]],
            [max[0], max[1], min[2]],
            [max[0], max[1], max[2]],
        ],
    ];
    for (face_index, face) in faces.into_iter().enumerate() {
        let brightness = match face_index {
            0 => 1.0,
            1 => 0.48,
            4 | 5 => 0.75,
            _ => 0.85,
        };
        let local = face.map(|point| [point[0] - min[0], point[1] - min[1], point[2] - min[2]]);
        let origin = [min[0], min[1], min[2]];
        let shaded = tint.map(|channel| channel * brightness);
        append_tinted_face(vertices, local, origin, shaded);
    }
}

fn build_mob_vertices(mobs: &[Mob]) -> Vec<Vertex> {
    let mut vertices = Vec::with_capacity(mobs.len() * MOB_VERTEX_COUNT);
    for mob in mobs {
        let position = if mob.alive {
            mob.position
        } else {
            Vector3::new(0.0, -1000.0, 0.0)
        };
        let x = position.x;
        let y = position.y;
        let z = position.z;
        let tint = mob_color(mob.kind);
        append_mob_box(
            &mut vertices,
            [x - 0.30, y, z - 0.22],
            [x + 0.30, y + 0.70, z + 0.22],
            tint,
        );
        append_mob_box(
            &mut vertices,
            [x - 0.24, y + 0.68, z - 0.24],
            [x + 0.24, y + 1.16, z + 0.24],
            tint.map(|channel| (channel * 1.18).min(1.0)),
        );
        append_mob_box(
            &mut vertices,
            [x - 0.35, y + 1.27, z - 0.04],
            [x + 0.35, y + 1.34, z + 0.04],
            [0.12, 0.08, 0.08],
        );
        let health_width = 0.66 * mob.health as f32 / 8.0;
        if mob.alive && health_width > 0.0 {
            append_mob_box(
                &mut vertices,
                [x - 0.33, y + 1.28, z - 0.06],
                [x - 0.33 + health_width, y + 1.33, z + 0.06],
                [0.92, 0.12, 0.12],
            );
        } else {
            append_mob_box(
                &mut vertices,
                [x - 0.33, y + 1.28, z - 0.06],
                [x - 0.32, y + 1.33, z + 0.06],
                [0.92, 0.12, 0.12],
            );
        }
    }
    vertices
}

fn ray_hits_mob(
    chunk: &Chunk,
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    mobs: &[Mob],
    max_distance: f32,
) -> Option<usize> {
    mobs.iter()
        .enumerate()
        .filter(|(_, mob)| mob.alive)
        .filter_map(|(index, mob)| {
            let center = mob.position + Vector3::new(0.0, 0.7, 0.0);
            let to_center = center - origin;
            let distance = to_center.dot(direction);
            if !(0.0..=max_distance).contains(&distance) {
                return None;
            }
            let closest = origin + direction * distance;
            let miss = (center - closest).magnitude();
            if miss > 0.55 {
                return None;
            }
            let mut ray_distance = 0.0;
            while ray_distance < distance {
                let point = origin + direction * ray_distance;
                if chunk.get(
                    point.x.floor() as i32,
                    point.y.floor() as i32,
                    point.z.floor() as i32,
                ) != AIR
                {
                    return None;
                }
                ray_distance += 0.05;
            }
            Some((index, distance))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

fn mob_collides(chunk: &Chunk, position: Vector3<f32>) -> bool {
    let min_x = (position.x - 0.3).floor() as i32;
    let max_x = (position.x + 0.3).floor() as i32;
    let min_z = (position.z - 0.3).floor() as i32;
    let max_z = (position.z + 0.3).floor() as i32;
    let min_y = position.y.floor() as i32;
    let max_y = (position.y + 1.5).floor() as i32;
    (min_x..=max_x)
        .any(|x| (min_y..=max_y).any(|y| (min_z..=max_z).any(|z| chunk.get(x, y, z) != AIR)))
}

fn update_mobs(
    mobs: &mut [Mob],
    chunk: &Chunk,
    player: Vector3<f32>,
    elapsed: f32,
    survival: &mut SurvivalState,
) -> bool {
    let mut changed = false;
    for mob in mobs.iter_mut().filter(|mob| mob.alive) {
        mob.attack_timer = (mob.attack_timer - elapsed).max(0.0);
        let offset = Vector3::new(player.x - mob.position.x, 0.0, player.z - mob.position.z);
        let distance = offset.magnitude();
        if distance <= 1.2 && (player.y - mob.position.y).abs() <= 2.0 {
            if mob.attack_timer == 0.0 {
                survival.damage(2);
                mob.attack_timer = 1.5;
                changed = true;
            }
        } else if (1.2..8.0).contains(&distance) {
            let displacement = offset / distance * (1.1 * elapsed);
            let candidate = mob.position + displacement;
            if !mob_collides(chunk, candidate) {
                mob.position = candidate;
                changed = true;
            }
        }
    }
    changed
}

fn block_intersects_player(block: [i32; 3], player: Vector3<f32>) -> bool {
    let min_player = [
        player.x - PLAYER_RADIUS,
        player.y - PLAYER_EYE_HEIGHT,
        player.z - PLAYER_RADIUS,
    ];
    let max_player = [
        player.x + PLAYER_RADIUS,
        player.y - 0.15,
        player.z + PLAYER_RADIUS,
    ];
    (0..3).all(|axis| {
        let min_block = block[axis] as f32;
        let max_block = min_block + 1.0;
        min_player[axis] < max_block && max_player[axis] > min_block
    })
}

fn block_for_hotbar_slot(slot: usize) -> u8 {
    [
        JUNGLE_LOG,
        JUNGLE_LEAVES,
        OAK_PLANKS,
        RED_SAND,
        RED_SANDSTONE,
        SNOW_BLOCK,
        BLUE_ICE,
        MAGENTA_CRYSTAL,
        CRYSTAL,
    ][slot % 9]
}

fn surface_height(chunk: &Chunk, x: usize, z: usize) -> usize {
    (0..CHUNK_HEIGHT)
        .rev()
        .find(|&y| chunk.get(x as i32, y as i32, z as i32) != AIR)
        .map(|y| y + 1)
        .unwrap_or(1)
}

fn configure_cursor(window: &glutin::window::Window, capture: bool) -> Result<(), String> {
    if capture {
        window
            .set_cursor_grab(glutin::window::CursorGrabMode::Confined)
            .or_else(|_| window.set_cursor_grab(glutin::window::CursorGrabMode::Locked))
            .map_err(|error| format!("could not capture mouse: {error}"))?;
        window.set_cursor_visible(false);
    } else {
        window
            .set_cursor_grab(glutin::window::CursorGrabMode::None)
            .map_err(|error| format!("could not release mouse: {error}"))?;
        window.set_cursor_visible(true);
    }
    Ok(())
}

fn player_collides(chunk: &Chunk, position: Vector3<f32>) -> bool {
    let min_x = (position.x - PLAYER_RADIUS).floor() as i32;
    let max_x = (position.x + PLAYER_RADIUS).floor() as i32;
    let min_z = (position.z - PLAYER_RADIUS).floor() as i32;
    let max_z = (position.z + PLAYER_RADIUS).floor() as i32;
    let min_y = (position.y - PLAYER_EYE_HEIGHT).floor() as i32;
    let max_y = (position.y - 0.15).floor() as i32;

    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                if chunk.get(x, y, z) != AIR {
                    return true;
                }
            }
        }
    }
    false
}

fn move_player(chunk: &Chunk, position: Vector3<f32>, displacement: Vector3<f32>) -> Vector3<f32> {
    let mut moved = position;
    for (axis, amount) in [
        (0, displacement.x),
        (1, displacement.y),
        (2, displacement.z),
    ] {
        let mut candidate = moved;
        match axis {
            0 => candidate.x += amount,
            1 => candidate.y += amount,
            _ => candidate.z += amount,
        }
        if !player_collides(chunk, candidate) {
            moved = candidate;
        }
    }
    moved
}

fn raycast_block(
    chunk: &Chunk,
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    max_distance: f32,
) -> Option<([i32; 3], [i32; 3])> {
    let mut previous = [
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    ];
    let mut distance = 0.0;
    while distance <= max_distance {
        let point = origin + direction * distance;
        let current = [
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        ];
        if chunk.get(current[0], current[1], current[2]) != AIR {
            return Some((current, previous));
        }
        previous = current;
        distance += 0.05;
    }
    None
}

fn flatten_area(
    chunk: &mut Chunk,
    x_range: std::ops::Range<usize>,
    z_range: std::ops::Range<usize>,
) -> usize {
    let surface = x_range
        .clone()
        .flat_map(|x| z_range.clone().map(move |z| (x, z)))
        .map(|(x, z)| {
            (0..CHUNK_HEIGHT)
                .rev()
                .find(|&y| chunk.get(x as i32, y as i32, z as i32) != AIR)
                .map(|y| y + 1)
                .unwrap_or(1)
        })
        .max()
        .unwrap_or(4);
    for x in x_range {
        for z in z_range.clone() {
            for y in 0..CHUNK_HEIGHT {
                if y + 1 < surface {
                    chunk.set(x, y, z, if y == 0 { STONE } else { DIRT });
                } else if y + 1 == surface {
                    chunk.set(x, y, z, GRASS_BLOCK);
                } else {
                    chunk.set(x, y, z, AIR);
                }
            }
        }
    }
    surface
}

fn build_house(chunk: &mut Chunk, x: usize, z: usize, surface: usize) {
    let width = 4usize;
    for local_x in 0..width {
        for local_z in 0..width {
            chunk.set(x + local_x, surface, z + local_z, OAK_PLANKS);
            for wall_y in 1..=3 {
                let edge =
                    local_x == 0 || local_x == width - 1 || local_z == 0 || local_z == width - 1;
                if !edge {
                    continue;
                }
                let doorway = local_x == 1 && local_z == width - 1 && wall_y <= 2;
                if doorway {
                    continue;
                }
                let window = wall_y == 2
                    && ((local_x == 0 && local_z == 1)
                        || (local_x == width - 1 && local_z == 2)
                        || (local_z == 0 && local_x == 2));
                let corner = (local_x == 0 || local_x == width - 1)
                    && (local_z == 0 || local_z == width - 1);
                chunk.set(
                    x + local_x,
                    surface + wall_y,
                    z + local_z,
                    if window {
                        GLASS
                    } else if corner {
                        OAK_LOG
                    } else if wall_y == 1 {
                        COBBLESTONE
                    } else {
                        OAK_PLANKS
                    },
                );
            }
        }
    }

    for roof_x in 0..width {
        for roof_z in 0..width {
            chunk.set(x + roof_x, surface + 4, z + roof_z, OAK_PLANKS);
        }
    }
}

fn build_village(chunk: &mut Chunk) {
    let first_surface = flatten_area(chunk, 2..6, 2..6);
    let second_surface = flatten_area(chunk, 9..13, 2..6);
    build_house(chunk, 2, 2, first_surface);
    build_house(chunk, 9, 2, second_surface);

    flatten_area(chunk, 0..CHUNK_WIDTH, 6..10);
    for x in 0..CHUNK_WIDTH {
        let surface = (0..CHUNK_HEIGHT)
            .rev()
            .find(|&y| chunk.get(x as i32, y as i32, 8) != AIR)
            .map(|y| y + 1)
            .unwrap_or(1);
        chunk.set(x, surface - 1, 8, PATH_BLOCK);
        if x % 4 == 0 {
            chunk.set(x, surface, 8, OAK_PLANKS);
        }
    }
    for x in [4usize, 11] {
        for z in 6..=8 {
            let surface = (0..CHUNK_HEIGHT)
                .rev()
                .find(|&y| chunk.get(x as i32, y as i32, z as i32) != AIR)
                .map(|y| y + 1)
                .unwrap_or(1);
            chunk.set(x, surface - 1, z, PATH_BLOCK);
        }
    }
}

fn build_snow_pine(chunk: &mut Chunk, x: usize, z: usize, seed: u32) {
    let ground = surface_height(chunk, x, z);
    let trunk_height = 4 + (coordinate_hash(x, z, seed) % 3) as usize;
    for y in ground..(ground + trunk_height).min(CHUNK_HEIGHT) {
        chunk.set(x, y, z, SPRUCE_LOG);
    }

    for layer in 0..4 {
        let radius = 2usize.saturating_sub(layer / 2);
        let y = ground + trunk_height - 2 + layer;
        if y >= CHUNK_HEIGHT {
            continue;
        }
        for leaf_x in x.saturating_sub(radius)..=(x + radius).min(CHUNK_WIDTH - 1) {
            for leaf_z in z.saturating_sub(radius)..=(z + radius).min(CHUNK_DEPTH - 1) {
                if leaf_x.abs_diff(x) + leaf_z.abs_diff(z) <= radius + 1
                    && chunk.get(leaf_x as i32, y as i32, leaf_z as i32) == AIR
                {
                    chunk.set(leaf_x, y, leaf_z, SPRUCE_LEAVES);
                }
            }
        }
    }
    if ground + trunk_height + 2 < CHUNK_HEIGHT {
        chunk.set(x, ground + trunk_height + 1, z, SNOW_BLOCK);
    }
}

fn build_jungle_tree(chunk: &mut Chunk, x: usize, z: usize, seed: u32) {
    let ground = surface_height(chunk, x, z);
    let trunk_height = 6 + (coordinate_hash(x, z, seed) % 3) as usize;
    for y in ground..(ground + trunk_height).min(CHUNK_HEIGHT) {
        chunk.set(x, y, z, JUNGLE_LOG);
    }

    let crown = ground + trunk_height - 1;
    for layer in 0..4 {
        let radius = if layer == 0 { 2 } else { 3 };
        let y = crown + layer;
        if y >= CHUNK_HEIGHT {
            continue;
        }
        for leaf_x in x.saturating_sub(radius)..=(x + radius).min(CHUNK_WIDTH - 1) {
            for leaf_z in z.saturating_sub(radius)..=(z + radius).min(CHUNK_DEPTH - 1) {
                let edge = leaf_x.abs_diff(x) == radius && leaf_z.abs_diff(z) == radius;
                let sparse_corner = edge && coordinate_hash(leaf_x, leaf_z, seed) % 3 != 0;
                if !sparse_corner && chunk.get(leaf_x as i32, y as i32, leaf_z as i32) == AIR {
                    chunk.set(leaf_x, y, leaf_z, JUNGLE_LEAVES);
                }
            }
        }
    }
}

fn build_jungle_hut(chunk: &mut Chunk) {
    let x = 0;
    let z = 5;
    let width = 4;
    let surface = flatten_area(chunk, x..x + width, z..z + width);
    for local_x in 0..width {
        for local_z in 0..width {
            chunk.set(x + local_x, surface, z + local_z, JUNGLE_LOG);
            for wall_y in 1..=3 {
                let edge =
                    local_x == 0 || local_x == width - 1 || local_z == 0 || local_z == width - 1;
                if !edge {
                    continue;
                }
                let doorway = local_x == 1 && local_z == width - 1 && wall_y <= 2;
                if doorway {
                    continue;
                }
                let window = wall_y == 2
                    && ((local_x == 0 && local_z == 1) || (local_z == 0 && local_x == 2));
                chunk.set(
                    x + local_x,
                    surface + wall_y,
                    z + local_z,
                    if window {
                        GLASS
                    } else if local_x == 0
                        || local_x == width - 1
                        || local_z == 0
                        || local_z == width - 1
                    {
                        JUNGLE_LOG
                    } else {
                        OAK_PLANKS
                    },
                );
            }
            chunk.set(x + local_x, surface + 4, z + local_z, JUNGLE_LEAVES);
        }
    }
}

fn build_red_pyramid(chunk: &mut Chunk, x: usize, z: usize, size: usize) {
    let surface = flatten_area(chunk, x..x + size, z..z + size);
    let base = surface - 1;
    for px in x..x + size {
        for pz in z..z + size {
            chunk.set(px, base, pz, RED_SAND);
        }
    }
    for layer in 0..size.div_ceil(2) {
        let min_x = x + layer;
        let max_x = x + size - 1 - layer;
        let min_z = z + layer;
        let max_z = z + size - 1 - layer;
        let y = base + layer + 1;
        if y >= CHUNK_HEIGHT {
            break;
        }
        for px in min_x..=max_x {
            for pz in min_z..=max_z {
                chunk.set(px, y, pz, RED_SANDSTONE);
            }
        }
    }

    let entrance_x = x + size / 2;
    let center_z = z + size / 2;
    for z_pos in center_z..z + size {
        for y in base + 1..=base + 2 {
            chunk.set(entrance_x, y, z_pos, AIR);
        }
    }
    for px in entrance_x - 1..=entrance_x + 1 {
        for pz in center_z - 1..=center_z + 1 {
            for y in base + 1..=base + 2 {
                chunk.set(px, y, pz, AIR);
            }
        }
    }
    chunk.set(
        entrance_x,
        base + size.div_ceil(2),
        center_z,
        MAGENTA_CRYSTAL,
    );
}

fn build_ice_spike(chunk: &mut Chunk, x: usize, z: usize, height: usize) {
    let base = surface_height(chunk, x, z);
    for layer in 0..height {
        let y = base + layer;
        if y >= CHUNK_HEIGHT {
            break;
        }
        let radius = if layer < 2 {
            1
        } else {
            (height - layer).div_ceil(4).min(2)
        };
        let block = if layer + 1 == height {
            MAGENTA_CRYSTAL
        } else if layer + 4 >= height {
            BLUE_ICE
        } else {
            ICE
        };
        let min_x = x.saturating_sub(radius);
        let max_x = (x + radius).min(CHUNK_WIDTH - 1);
        let min_z = z.saturating_sub(radius);
        let max_z = (z + radius).min(CHUNK_DEPTH - 1);
        for px in min_x..=max_x {
            for pz in min_z..=max_z {
                if px.abs_diff(x) + pz.abs_diff(z) <= radius + 1 {
                    chunk.set(px, y, pz, block);
                }
            }
        }
    }
}

fn build_frontier(chunk: &mut Chunk, seed: u32) {
    build_jungle_tree(chunk, 1, 13, seed);
    build_jungle_tree(chunk, 4, 2, seed ^ 0xA511_E9B3);
    build_jungle_hut(chunk);
    build_red_pyramid(chunk, 5, 4, 6);
    build_rift_shrine(chunk, 10, 1, seed);
    for (x, z, height) in [(12, 3, 10), (14, 7, 15), (12, 12, 11), (15, 14, 8)] {
        build_ice_spike(chunk, x, z, height);
    }
    for x in 0..CHUNK_WIDTH {
        let y = terrain_height(x, 8, WorldStyle::Frontier, seed).saturating_sub(1);
        chunk.set(x, y, 8, PATH_BLOCK);
    }
}

fn build_rift_shrine(chunk: &mut Chunk, x: usize, z: usize, seed: u32) {
    let surface = flatten_area(chunk, x..x + 5, z..z + 4);
    let base = surface - 1;
    for px in x..x + 5 {
        for pz in z..z + 4 {
            chunk.set(px, base, pz, RED_SANDSTONE);
        }
    }

    for (px, pz) in [(x, z), (x + 4, z), (x, z + 3), (x + 4, z + 3)] {
        let height = 2 + (coordinate_hash(px, pz, seed) % 2) as usize;
        for y in base + 1..=base + height {
            chunk.set(px, y, pz, ICE_BRICKS);
        }
        chunk.set(px, base + height + 1, pz, MAGENTA_CRYSTAL);
    }
    for px in x + 1..x + 4 {
        chunk.set(px, base + 1, z + 1, RED_SANDSTONE);
    }
    chunk.set(x + 2, base + 2, z + 1, MAGENTA_CRYSTAL);
}

fn build_ice_temple(chunk: &mut Chunk) {
    let surface = flatten_area(chunk, 3..13, 3..13);
    let base = surface - 1;
    for x in 3..13 {
        for z in 3..13 {
            chunk.set(x, base, z, PACKED_ICE);
        }
    }

    let wall_bottom = base + 1;
    for y in wall_bottom..wall_bottom + 6 {
        for x in 4..=12 {
            for z in 4..=12 {
                let wall = x == 4 || x == 12 || z == 4 || z == 12;
                if !wall {
                    continue;
                }
                let gate = z == 12 && x == 8 && y < wall_bottom + 3;
                let window = y == wall_bottom + 3
                    && ((x == 4 && z == 8) || (x == 12 && z == 8) || (z == 4 && x == 8));
                if gate {
                    continue;
                }
                chunk.set(
                    x,
                    y,
                    z,
                    if window {
                        ICE
                    } else if y == wall_bottom + 5 {
                        SNOW_BLOCK
                    } else {
                        ICE_BRICKS
                    },
                );
            }
        }
    }

    for (center_x, center_z) in [(5usize, 5usize), (11, 5), (5, 11), (11, 11)] {
        for layer in 0..8 {
            let radius = if layer < 5 { 1 } else { 0 };
            let block = if layer < 4 {
                ICE_BRICKS
            } else if layer < 7 {
                BLUE_ICE
            } else {
                CRYSTAL
            };
            let y = wall_bottom + layer;
            for x in center_x - radius..=center_x + radius {
                for z in center_z - radius..=center_z + radius {
                    chunk.set(x, y, z, block);
                }
            }
        }
    }

    for y in wall_bottom + 5..wall_bottom + 14 {
        let block = if y >= wall_bottom + 11 {
            CRYSTAL
        } else if y >= wall_bottom + 8 {
            BLUE_ICE
        } else {
            ICE_BRICKS
        };
        for x in 7..=9 {
            for z in 7..=9 {
                let edge = x == 7 || x == 9 || z == 7 || z == 9;
                if edge {
                    chunk.set(x, y, z, block);
                }
            }
        }
    }
    chunk.set(8, wall_bottom + 14, 8, CRYSTAL);

    for z in 13..15 {
        for x in 7..=9 {
            chunk.set(x, base, z, ICE);
        }
    }
    for x in 7..=9 {
        chunk.set(x, base, 3, SNOW_BLOCK);
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

fn frontier_biome(x: usize) -> Biome {
    match x {
        0..=4 => Biome::Jungle,
        5..=9 => Biome::RedDesert,
        _ => Biome::FrostTundra,
    }
}

fn frontier_top_block(x: usize, z: usize, seed: u32) -> u8 {
    let biome = frontier_biome(x);
    let transitional = matches!(x, 4 | 5 | 9 | 10) && coordinate_hash(x, z, seed) % 3 == 0;
    if transitional {
        return match biome {
            Biome::Jungle => RED_SAND,
            Biome::RedDesert => {
                if x == 5 {
                    GRASS_BLOCK
                } else {
                    SNOW_BLOCK
                }
            }
            Biome::FrostTundra => RED_SAND,
        };
    }
    match biome {
        Biome::Jungle => GRASS_BLOCK,
        Biome::RedDesert => RED_SAND,
        Biome::FrostTundra => SNOW_BLOCK,
    }
}

fn terrain_block(x: usize, y: usize, z: usize, height: usize, style: WorldStyle, seed: u32) -> u8 {
    if y + 1 == height {
        return if style == WorldStyle::Frontier {
            frontier_top_block(x, z, seed)
        } else if style == WorldStyle::Frostlands {
            SNOW_BLOCK
        } else {
            GRASS_BLOCK
        };
    }
    if y + 3 < height {
        return STONE;
    }
    match style {
        WorldStyle::Frostlands => PACKED_ICE,
        WorldStyle::Frontier => match frontier_biome(x) {
            Biome::Jungle => DIRT,
            Biome::RedDesert if y + 1 == height - 1 => RED_SAND,
            Biome::RedDesert => RED_SANDSTONE,
            Biome::FrostTundra => PACKED_ICE,
        },
        WorldStyle::Hills | WorldStyle::Flat => DIRT,
    }
}

fn terrain_height(x: usize, z: usize, style: WorldStyle, seed: u32) -> usize {
    match style {
        WorldStyle::Flat => 4,
        WorldStyle::Hills => {
            let broad = ((x / 4 + z / 4 + seed as usize) % 3) as isize;
            let detail = (coordinate_hash(x, z, seed) % 3) as isize - 1;
            (5 + broad + detail).clamp(3, 8) as usize
        }
        WorldStyle::Frostlands => {
            let broad = ((x / 3 + z / 4 + seed as usize) % 4) as isize;
            let detail = (coordinate_hash(x, z, seed) % 3) as isize - 1;
            (6 + broad + detail).clamp(4, 10) as usize
        }
        WorldStyle::Frontier => {
            let base = match x {
                0..=4 => 7,
                5 => 6,
                6..=9 => 5,
                10 => 6,
                _ => 8,
            };
            let broad = ((z / 4 + seed as usize) % 3) as isize - 1;
            let detail = (coordinate_hash(x, z, seed) % 3) as isize - 1;
            (base + broad + detail).clamp(3, 12) as usize
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
        OAK_PLANKS => "oak_planks.png",
        COBBLESTONE => "cobblestone.png",
        GLASS => "glass.png",
        PATH_BLOCK => "path.png",
        SNOW_BLOCK => "snow_block.png",
        ICE => "ice.png",
        PACKED_ICE => "packed_ice.png",
        BLUE_ICE => "blue_ice.png",
        SPRUCE_LOG => "spruce_log.png",
        SPRUCE_LEAVES => "spruce_leaves.png",
        ICE_BRICKS => "ice_bricks.png",
        CRYSTAL => "crystal.png",
        JUNGLE_LOG => "jungle_log.png",
        JUNGLE_LEAVES => "jungle_leaves.png",
        RED_SAND => "red_sand.png",
        RED_SANDSTONE => "red_sandstone.png",
        MAGENTA_CRYSTAL => "magenta_crystal.png",
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
        OAK_PLANKS => [0.58, 0.39, 0.21],
        COBBLESTONE => [0.43, 0.45, 0.46],
        GLASS => [0.66, 0.84, 0.88],
        PATH_BLOCK => [0.58, 0.53, 0.40],
        SNOW_BLOCK => [0.86, 0.92, 0.98],
        ICE => [0.58, 0.82, 0.95],
        PACKED_ICE => [0.38, 0.61, 0.85],
        BLUE_ICE => [0.22, 0.47, 0.82],
        SPRUCE_LOG => [0.31, 0.23, 0.16],
        SPRUCE_LEAVES => [0.16, 0.34, 0.32],
        ICE_BRICKS => [0.49, 0.64, 0.82],
        CRYSTAL => [0.53, 0.93, 1.0],
        JUNGLE_LOG => [0.36, 0.23, 0.13],
        JUNGLE_LEAVES => [0.08, 0.39, 0.16],
        RED_SAND => [0.72, 0.31, 0.19],
        RED_SANDSTONE => [0.61, 0.24, 0.14],
        MAGENTA_CRYSTAL => [0.88, 0.16, 0.78],
        REDSTONE_BLOCK => redstone_particle_color(),
        _ => [0.8, 0.0, 0.8],
    }
}

fn redstone_particle_color() -> [f32; 3] {
    [0.85, 0.08, 0.12]
}

fn append_face(
    vertices: &mut Vec<Vertex>,
    corners: [[f32; 3]; 4],
    origin: [i32; 3],
    brightness: f32,
) {
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
            color: [brightness, brightness, brightness],
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
                            match direction.face {
                                Face::Top => 1.0,
                                Face::Bottom => 0.52,
                                Face::Side => {
                                    if direction.offset[0] != 0 {
                                        0.78
                                    } else {
                                        0.86
                                    }
                                }
                            },
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

fn fallback_texture_data(block_id: u8) -> Vec<u8> {
    let base = block_fallback_color(block_id).map(|channel| (channel * 255.0) as i16);
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let hash = coordinate_hash(x, y, (block_id as u32).wrapping_mul(0x9E37_79B9));
            let mut variation = (hash % 15) as i16 - 7;
            match block_id {
                ICE_BRICKS if x % 8 == 0 || y % 8 == 0 => variation -= 32,
                CRYSTAL if x == y || x + y == 15 => variation += 22,
                MAGENTA_CRYSTAL if x == y || x + y == 15 => variation += 35,
                OAK_LOG | SPRUCE_LOG | JUNGLE_LOG if x % 5 == 0 => variation -= 15,
                OAK_LEAVES | SPRUCE_LEAVES | JUNGLE_LEAVES if hash % 4 == 0 => variation += 13,
                SNOW_BLOCK if hash % 13 == 0 => variation += 10,
                ICE | PACKED_ICE | BLUE_ICE if (x + y * 3) % 13 == 0 => variation += 12,
                RED_SAND if hash % 5 == 0 => variation += 12,
                RED_SANDSTONE if x % 8 == 0 || y % 8 == 0 => variation -= 20,
                _ => {}
            }
            for channel in base {
                pixels.push((channel + variation).clamp(0, 255) as u8);
            }
            pixels.push(255);
        }
    }
    pixels
}

fn fallback_texture(display: &glium::Display, block_id: u8) -> glium::texture::SrgbTexture2d {
    let raw = glium::texture::RawImage2d::from_raw_rgba(fallback_texture_data(block_id), (16, 16));
    glium::texture::SrgbTexture2d::new(display, raw)
        .expect("create procedural block fallback texture")
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
        MenuPage::Main => vec![
            "SINGLEPLAYER",
            "MULTIPLAYER",
            "MODS & ADD-ONS",
            "SETTINGS",
            "EXIT",
        ],
        MenuPage::Worlds => vec![
            "PREVIOUS WORLD",
            "NEXT WORLD",
            "CREATE WORLD",
            "PLAY WORLD",
            "DELETE WORLD",
            "BACK",
        ],
        MenuPage::Modes => vec!["SURVIVAL", "CREATIVE", "BACK"],
        MenuPage::Settings => vec![
            match world_style {
                WorldStyle::Hills => "WORLD HILLS",
                WorldStyle::Flat => "WORLD FLAT",
                WorldStyle::Frostlands => "WORLD FROSTLANDS",
                WorldStyle::Frontier => "WORLD FRONTIER",
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
        MenuPage::Dead => vec!["RESPAWN", "MAIN MENU"],
    }
}

fn menu_layout(width: f32, height: f32, labels: &[&'static str]) -> Vec<MenuButton> {
    let button_width = 278.0_f32.min((width - 32.0).max(0.0));
    let button_height = 42.0_f32.min((height - 24.0).max(0.0));
    let panel_width = button_width + 48.0;
    let button_x = (panel_width - button_width) * 0.5;
    let rows_height =
        labels.len() as f32 * button_height + labels.len().saturating_sub(1) as f32 * 13.0;
    let preferred_top = if labels.len() >= 6 { 138.0 } else { 130.0 };
    let start_y = (height * 0.5 - rows_height * 0.5)
        .max(preferred_top)
        .min(height * 0.55);

    labels
        .iter()
        .enumerate()
        .map(|(row, label)| MenuButton {
            label: *label,
            x: button_x,
            y: (start_y + row as f32 * (button_height + 13.0))
                .min((height - button_height - 12.0).max(0.0)),
            width: button_width,
            height: button_height,
        })
        .collect()
}

fn biome_label(style: WorldStyle, x: f32) -> &'static str {
    match style {
        WorldStyle::Hills => "GREEN HILLS",
        WorldStyle::Flat => "OPEN PLAINS",
        WorldStyle::Frostlands => "FROSTLANDS",
        WorldStyle::Frontier => match frontier_biome(x.floor().max(0.0) as usize) {
            Biome::Jungle => "JUNGLE",
            Biome::RedDesert => "RED SAND DESERT",
            Biome::FrostTundra => "FROZEN TUNDRA",
        },
    }
}

fn frontier_fog_color(x: f32) -> [f32; 3] {
    let jungle = [0.39, 0.65, 0.49];
    let desert = [0.80, 0.59, 0.43];
    let tundra = [0.54, 0.70, 0.86];
    if x < 4.0 {
        jungle
    } else if x < 6.0 {
        let blend = ((x - 4.0) / 2.0).clamp(0.0, 1.0);
        std::array::from_fn(|index| jungle[index] + (desert[index] - jungle[index]) * blend)
    } else if x < 9.0 {
        desert
    } else if x < 11.0 {
        let blend = ((x - 9.0) / 2.0).clamp(0.0, 1.0);
        std::array::from_fn(|index| desert[index] + (tundra[index] - desert[index]) * blend)
    } else {
        tundra
    }
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
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '&' => [12, 18, 20, 8, 21, 18, 13],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        _ => [0; 7],
    }
}

fn push_ui_rect(vertices: &mut Vec<UiVertex>, rect: [f32; 4], color: [f32; 3], screen: [f32; 2]) {
    push_ui_rect_alpha(vertices, rect, color, 1.0, screen);
}

fn push_ui_rect_alpha(
    vertices: &mut Vec<UiVertex>,
    rect: [f32; 4],
    color: [f32; 3],
    alpha: f32,
    screen: [f32; 2],
) {
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
    vertices.extend(corners.into_iter().map(|position| UiVertex {
        position,
        color,
        alpha,
    }));
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

fn push_pixel_icon(
    vertices: &mut Vec<UiVertex>,
    x: f32,
    y: f32,
    pixels: &[&str],
    scale: f32,
    color: [f32; 3],
    screen: [f32; 2],
) {
    for (row, line) in pixels.iter().enumerate() {
        for (column, pixel) in line.bytes().enumerate() {
            if pixel == b'#' {
                push_ui_rect(
                    vertices,
                    [
                        x + column as f32 * scale,
                        y + row as f32 * scale,
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

fn build_hud_vertices(
    width: f32,
    height: f32,
    health: u8,
    food: u8,
    selected_slot: usize,
    notice: Option<&str>,
    biome: &str,
    berries: u8,
    threats: usize,
    defeats: u32,
) -> Vec<UiVertex> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let screen = [width, height];
    let mut vertices = Vec::new();
    let heart = ["##..##", "######", "######", ".####.", "..##.."];
    let food_icon = ["..##..", ".####.", "######", ".####.", "..##.."];
    let icon_scale = 2.0;
    let icon_width = 12.0;
    let icon_gap = 5.0;
    let group_width = icon_width * 10.0 + icon_gap * 9.0;
    let status_width = group_width * 2.0 + 16.0;
    let status_y = height - 90.0;
    let start_x = (width - status_width) * 0.5;

    for index in 0..10 {
        let filled = index * 2 < health as usize;
        push_pixel_icon(
            &mut vertices,
            start_x + index as f32 * (icon_width + icon_gap),
            status_y,
            &heart,
            icon_scale,
            if filled {
                [0.92, 0.16, 0.19]
            } else {
                [0.24, 0.12, 0.14]
            },
            screen,
        );
        push_pixel_icon(
            &mut vertices,
            start_x + group_width + 16.0 + index as f32 * (icon_width + icon_gap),
            status_y,
            &food_icon,
            icon_scale,
            if index * 2 < food as usize {
                [0.83, 0.48, 0.17]
            } else {
                [0.27, 0.20, 0.14]
            },
            screen,
        );
    }

    let slot_size = 38.0;
    let slot_gap = 3.0;
    let bar_width = slot_size * 9.0 + slot_gap * 8.0;
    let bar_x = (width - bar_width) * 0.5;
    let bar_y = height - slot_size - 13.0;
    for slot in 0..9 {
        let x = bar_x + slot as f32 * (slot_size + slot_gap);
        push_ui_rect(
            &mut vertices,
            [x - 2.0, bar_y - 2.0, slot_size + 4.0, slot_size + 4.0],
            if slot == selected_slot % 9 {
                [0.93, 0.83, 0.47]
            } else {
                [0.04, 0.06, 0.07]
            },
            screen,
        );
        push_ui_rect(
            &mut vertices,
            [x, bar_y, slot_size, slot_size],
            [0.16, 0.18, 0.19],
            screen,
        );
        let item_color = block_fallback_color(block_for_hotbar_slot(slot));
        for pixel_y in 0..5 {
            for pixel_x in 0..5 {
                let noise = coordinate_hash(pixel_x, pixel_y, slot as u32 + 11) % 4;
                let scale = if noise == 0 {
                    0.68
                } else if noise == 1 {
                    0.84
                } else {
                    1.0
                };
                let color = item_color.map(|channel| (channel * scale).clamp(0.0, 1.0));
                push_ui_rect(
                    &mut vertices,
                    [
                        x + 7.0 + pixel_x as f32 * 4.0,
                        bar_y + 7.0 + pixel_y as f32 * 4.0,
                        4.0,
                        4.0,
                    ],
                    color,
                    screen,
                );
            }
        }
        push_ui_text(
            &mut vertices,
            match slot {
                0 => "1",
                1 => "2",
                2 => "3",
                3 => "4",
                4 => "5",
                5 => "6",
                6 => "7",
                7 => "8",
                _ => "9",
            },
            x + 5.0,
            bar_y + 5.0,
            [0.79, 0.84, 0.82],
            screen,
        );
    }

    let center = [width * 0.5, height * 0.5];
    push_ui_rect(
        &mut vertices,
        [center[0] - 1.0, center[1] - 8.0, 2.0, 16.0],
        [0.95, 0.98, 0.95],
        screen,
    );
    push_ui_rect(
        &mut vertices,
        [center[0] - 8.0, center[1] - 1.0, 16.0, 2.0],
        [0.95, 0.98, 0.95],
        screen,
    );
    push_ui_rect(
        &mut vertices,
        [12.0, 12.0, 224.0, 22.0],
        [0.035, 0.055, 0.075],
        screen,
    );
    push_ui_text(
        &mut vertices,
        "WASD MOVE ESC PAUSE",
        124.0,
        23.0,
        [0.91, 0.96, 0.93],
        screen,
    );
    push_ui_text(
        &mut vertices,
        &format!("F EAT: {berries}"),
        124.0,
        51.0,
        [0.96, 0.72, 0.42],
        screen,
    );
    push_ui_text(
        &mut vertices,
        &format!("THREATS {threats}  DEFEATS {defeats}"),
        width * 0.5,
        22.0,
        [0.98, 0.84, 0.72],
        screen,
    );
    let accent = match biome {
        "JUNGLE" => [0.24, 0.75, 0.36],
        "RED SAND DESERT" => [0.91, 0.42, 0.25],
        "FROZEN TUNDRA" | "FROSTLANDS" => [0.48, 0.77, 0.96],
        _ => [0.66, 0.82, 0.52],
    };
    let biome_x = (width - 194.0).max(12.0);
    push_ui_rect_alpha(
        &mut vertices,
        [biome_x, 12.0, 182.0, 26.0],
        [0.035, 0.055, 0.075],
        0.82,
        screen,
    );
    push_ui_rect(&mut vertices, [biome_x, 12.0, 4.0, 26.0], accent, screen);
    push_ui_text(
        &mut vertices,
        biome,
        biome_x + 96.0,
        29.0,
        [0.93, 0.97, 0.94],
        screen,
    );
    if let Some(notice) = notice {
        push_ui_rect_alpha(
            &mut vertices,
            [width * 0.5 - 140.0, 18.0, 280.0, 28.0],
            [0.12, 0.06, 0.04],
            0.88,
            screen,
        );
        push_ui_text(
            &mut vertices,
            notice,
            width * 0.5,
            32.0,
            [1.0, 0.78, 0.48],
            screen,
        );
    }
    vertices
}

fn build_menu_vertices(
    width: f32,
    height: f32,
    page: MenuPage,
    world_style: WorldStyle,
    speed_index: usize,
    mode: Option<GameMode>,
    world_name: Option<&str>,
    world_count: usize,
    notice: Option<&str>,
    health: u8,
    food: u8,
    selected_slot: usize,
    current_biome: &'static str,
    berries: u8,
    threats: usize,
    defeats: u32,
) -> Vec<UiVertex> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let screen = [width, height];
    let mut vertices = Vec::new();
    if page == MenuPage::Playing {
        return build_hud_vertices(
            width,
            height,
            health,
            food,
            selected_slot,
            notice,
            current_biome,
            berries,
            threats,
            defeats,
        );
    }

    let labels = menu_labels(page, world_style, speed_index);
    let panel_width = menu_layout(width, height, &labels)
        .first()
        .map(|button| button.x * 2.0 + button.width)
        .unwrap_or(0.0);
    let panel_width = panel_width.min(width);
    push_ui_rect_alpha(
        &mut vertices,
        [0.0, 0.0, panel_width + 24.0, height],
        [0.035, 0.055, 0.075],
        0.76,
        screen,
    );
    push_ui_rect(
        &mut vertices,
        [panel_width + 20.0, 0.0, 4.0, height],
        [0.22, 0.68, 0.48],
        screen,
    );
    let title_x = if page == MenuPage::Main {
        width * 0.5
    } else {
        (panel_width + 24.0) * 0.5
    };
    if page == MenuPage::Main {
        push_ui_text_scaled(
            &mut vertices,
            "MAIN MENU",
            width * 0.5,
            28.0,
            [0.97, 0.98, 0.96],
            screen,
            1.7,
        );
    }
    let title = match page {
        MenuPage::Main => "MAAZCRAFT",
        MenuPage::Worlds => "SINGLEPLAYER",
        MenuPage::Modes => "SELECT MODE",
        MenuPage::Settings => "SETTINGS",
        MenuPage::Pause => "PAUSED",
        MenuPage::Playing => "MAAZ CRAFT",
        MenuPage::Dead => "YOU WERE DEFEATED",
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
        (MenuPage::Worlds, _) => "SAVED WORLDS",
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
        if page == MenuPage::Main { 100.0 } else { 82.0 },
        [0.68, 0.76, 0.79],
        screen,
        if page == MenuPage::Main { 1.5 } else { 1.2 },
    );

    for button in menu_layout(width, height, &labels) {
        push_ui_rect_alpha(
            &mut vertices,
            [button.x, button.y, button.width, button.height],
            [0.055, 0.075, 0.09],
            0.90,
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
        push_ui_rect(
            &mut vertices,
            [button.x + 12.0, button.y + 11.0, 3.0, button.height - 22.0],
            [0.38, 0.86, 0.61],
            screen,
        );
        push_ui_text(
            &mut vertices,
            button.label,
            button.x + button.width * 0.5 + 6.0,
            button.y + button.height * 0.5,
            [0.91, 0.96, 0.93],
            screen,
        );
    }
    if page == MenuPage::Main {
        push_ui_rect_alpha(
            &mut vertices,
            [width - 205.0, height - 112.0, 188.0, 78.0],
            [0.035, 0.055, 0.075],
            0.72,
            screen,
        );
        push_ui_rect(
            &mut vertices,
            [width - 205.0, height - 112.0, 188.0, 3.0],
            [0.22, 0.68, 0.48],
            screen,
        );
        push_ui_text(
            &mut vertices,
            "WORLD NEWS",
            width - 111.0,
            height - 92.0,
            [0.91, 0.96, 0.93],
            screen,
        );
        push_ui_text(
            &mut vertices,
            "GREENVALE AWAITS",
            width - 111.0,
            height - 68.0,
            [0.68, 0.76, 0.79],
            screen,
        );
        push_ui_text(
            &mut vertices,
            "MAAZCRAFT 2.0",
            86.0,
            height - 16.0,
            [0.96, 0.97, 0.94],
            screen,
        );
        push_ui_text(
            &mut vertices,
            "GAME DEVELOPED BY MAAZ",
            width - 100.0,
            height - 16.0,
            [0.96, 0.97, 0.94],
            screen,
        );
    }
    if page == MenuPage::Worlds {
        push_ui_rect(
            &mut vertices,
            [width * 0.55, 72.0, width * 0.38, height - 112.0],
            [0.035, 0.055, 0.075],
            screen,
        );
        push_ui_text(
            &mut vertices,
            "WORLD PREVIEW",
            width * 0.74,
            96.0,
            [0.38, 0.86, 0.61],
            screen,
        );
        let selected = world_name.unwrap_or("NO WORLD SELECTED");
        push_ui_text(
            &mut vertices,
            selected,
            width * 0.74,
            132.0,
            [0.91, 0.96, 0.93],
            screen,
        );
        push_ui_text(
            &mut vertices,
            if world_count == 0 {
                "CREATE A WORLD TO PLAY"
            } else {
                match world_style {
                    WorldStyle::Hills => "HILLS AND VILLAGE",
                    WorldStyle::Flat => "FLAT VILLAGE",
                    WorldStyle::Frostlands => "FROSTLANDS ICE TEMPLE",
                    WorldStyle::Frontier => "JUNGLE / RED SAND / TUNDRA",
                }
            },
            width * 0.74,
            158.0,
            [0.68, 0.76, 0.79],
            screen,
        );
    }
    if let Some(notice) = notice {
        push_ui_text(
            &mut vertices,
            notice,
            title_x,
            height - 24.0,
            [0.97, 0.79, 0.40],
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
    biome: &'static str,
    speed_index: usize,
    mode: Option<GameMode>,
    world_name: Option<String>,
    world_count: usize,
    selected_index: usize,
    notice: Option<&'static str>,
    health: u8,
    food: u8,
    berries: u8,
    threats: usize,
    defeats: u32,
    selected_slot: usize,
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

    let save_path = match world_store_path() {
        Ok(path) => Some(path),
        Err(error) => {
            eprintln!("{error}");
            None
        }
    };
    let mut startup_notice = None;
    let mut storage_write_enabled = save_path.is_some();
    let mut worlds = match save_path.as_deref().map(load_worlds) {
        Some(Ok(worlds)) => worlds,
        Some(Err(error)) => {
            eprintln!("{error}");
            startup_notice = Some("WORLD SAVE COULD NOT BE LOADED");
            storage_write_enabled = false;
            Vec::new()
        }
        None => {
            startup_notice = Some("WORLD SAVE LOCATION UNAVAILABLE");
            storage_write_enabled = false;
            Vec::new()
        }
    };
    if worlds.is_empty() {
        worlds.push(WorldSave::new(
            "WILDBIOME".to_string(),
            WorldStyle::Frontier,
            12,
        ));
        if storage_write_enabled {
            let path = save_path
                .as_deref()
                .expect("enabled world storage has a path");
            if let Err(error) = save_worlds(path, &worlds) {
                eprintln!("{error}");
                startup_notice = Some("WORLD SAVE COULD NOT BE WRITTEN");
                storage_write_enabled = false;
            }
        }
    }
    let mut world_style = worlds[0].style;
    let mut selected_world = 0usize;
    let mut next_world_seed = worlds
        .iter()
        .map(|world| world.seed)
        .max()
        .unwrap_or(12)
        .wrapping_add(1);
    let mut game_mode = None;
    let mut active_chunk = worlds[0].chunk().expect("validated world save block count");
    let mut mesh_batches = build_chunk_mesh(&active_chunk);
    let mut vertex_batches: Vec<_> = mesh_batches
        .iter()
        .map(|batch| {
            glium::VertexBuffer::new(&display, &batch.vertices)
                .expect("upload chunk mesh vertex buffer")
        })
        .collect();

    let mut block_textures = BTreeMap::new();
    load_mesh_textures(&display, &mesh_batches, &mut block_textures);

    let fallback_textures: BTreeMap<_, _> = [
        STONE,
        GRASS_BLOCK,
        DIRT,
        OAK_LOG,
        OAK_LEAVES,
        OAK_PLANKS,
        COBBLESTONE,
        GLASS,
        PATH_BLOCK,
        SNOW_BLOCK,
        ICE,
        PACKED_ICE,
        BLUE_ICE,
        SPRUCE_LOG,
        SPRUCE_LEAVES,
        ICE_BRICKS,
        CRYSTAL,
        JUNGLE_LOG,
        JUNGLE_LEAVES,
        RED_SAND,
        RED_SANDSTONE,
        MAGENTA_CRYSTAL,
        REDSTONE_BLOCK,
    ]
    .into_iter()
    .map(|block_id| (block_id, fallback_texture(&display, block_id)))
    .collect();

    const VOXEL_VERTEX_SHADER: &str = r#"
        #version 120
        attribute vec3 position;
        attribute vec2 tex_coords;
        attribute vec3 color;
        varying vec2 v_tex_coords;
        varying vec3 v_color;
        varying vec3 v_world_pos;
        uniform mat4 matrix;
        void main() {
            v_tex_coords = tex_coords;
            v_color = color;
            v_world_pos = position;
            gl_Position = matrix * vec4(position, 1.0);
        }
    "#;

    const VOXEL_FRAGMENT_SHADER: &str = r#"
        #version 120
        varying vec2 v_tex_coords;
        varying vec3 v_color;
        varying vec3 v_world_pos;
        uniform sampler2D block_texture;
        uniform vec3 camera_position;
        uniform vec3 fog_color;
        uniform vec2 fog_range;
        void main() {
            vec4 texel = texture2D(block_texture, v_tex_coords);
            float fog = smoothstep(fog_range.x, fog_range.y, length(v_world_pos - camera_position));
            gl_FragColor = vec4(mix(texel.rgb * v_color, fog_color, fog), texel.a);
        }
    "#;

    const UI_VERTEX_SHADER: &str = r#"
        #version 120
        attribute vec2 position;
        attribute vec3 color;
        attribute float alpha;
        varying vec3 v_color;
        varying float v_alpha;
        void main() {
            v_color = color;
            v_alpha = alpha;
            gl_Position = vec4(position, 0.0, 1.0);
        }
    "#;

    const UI_FRAGMENT_SHADER: &str = r#"
        #version 120
        varying vec3 v_color;
        varying float v_alpha;
        void main() {
            gl_FragColor = vec4(v_color, v_alpha);
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
    let mut mobs = spawn_mobs(worlds[0].style, worlds[0].seed, &active_chunk);
    let mob_vertices = build_mob_vertices(&mobs);
    let mob_vertex_buffer = glium::VertexBuffer::dynamic(&display, &mob_vertices)
        .expect("create dynamic mob vertex buffer");
    let mob_texture = white_texture(&display);
    let mut mob_mesh_dirty = false;
    let mut survival = SurvivalState::new(worlds[0].berries);
    let mut defeats = 0u32;
    let mut attack_cooldown = 0.0f32;
    let mut page = MenuPage::Main;
    let mut settings_return_page = MenuPage::Main;
    let mut speed_index = 1usize;
    let mut notice: Option<&'static str> = startup_notice;
    let mut selected_slot = 0usize;
    let mut cursor_position = [0.0f32; 2];
    let mut pressed_keys = HashSet::new();
    let mut camera_position = Vector3::new(
        8.5,
        surface_height(&active_chunk, 8, 13) as f32 + PLAYER_EYE_HEIGHT,
        13.5,
    );
    let mut camera_yaw = std::f32::consts::PI;
    let mut camera_pitch = -0.2f32;
    let mut vertical_velocity = 0.0f32;
    let mut player_grounded = false;
    let mut previous_frame = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = glutin::event_loop::ControlFlow::Poll;
        match event {
            glutin::event::Event::DeviceEvent {
                event: glutin::event::DeviceEvent::MouseMotion { delta },
                ..
            } if page == MenuPage::Playing => {
                camera_yaw += delta.0 as f32 * 0.0025;
                camera_pitch = (camera_pitch - delta.1 as f32 * 0.0025).clamp(-1.45, 1.45);
            }
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
                } if page == MenuPage::Playing => {
                    let direction = Vector3::new(
                        camera_yaw.sin() * camera_pitch.cos(),
                        camera_pitch.sin(),
                        camera_yaw.cos() * camera_pitch.cos(),
                    );
                    if attack_cooldown <= 0.0 {
                        if let Some(target) =
                            ray_hits_mob(&active_chunk, camera_position, direction, &mobs, 4.0)
                        {
                            let mob = &mut mobs[target];
                            mob.health = mob.health.saturating_sub(4);
                            attack_cooldown = 0.35;
                            mob_mesh_dirty = true;
                            if mob.health == 0 {
                                mob.alive = false;
                                defeats += 1;
                                let berry_found = game_mode == Some(GameMode::Survival)
                                    && survival.harvest_berry();
                                if berry_found {
                                    worlds[selected_world].berries = survival.berries;
                                }
                                notice = Some(if berry_found {
                                    "THREAT DEFEATED: BERRY FOUND"
                                } else {
                                    "THREAT DEFEATED"
                                });
                                if berry_found {
                                    if let Err(error) = persist_worlds(
                                        save_path.as_deref(),
                                        storage_write_enabled,
                                        &worlds,
                                    ) {
                                        eprintln!("{error}");
                                        notice = Some("WORLD SAVE FAILED");
                                        storage_write_enabled = false;
                                    }
                                }
                            }
                        } else if let Some((hit, _)) =
                            raycast_block(&active_chunk, camera_position, direction, 5.0)
                        {
                            let mined_block = active_chunk.get(hit[0], hit[1], hit[2]);
                            if mined_block != AIR
                                && active_chunk.set_if_in_bounds(hit[0], hit[1], hit[2], AIR)
                            {
                                if game_mode == Some(GameMode::Survival)
                                    && matches!(mined_block, OAK_LEAVES | JUNGLE_LEAVES)
                                    && coordinate_hash(
                                        hit[0] as usize,
                                        hit[2] as usize,
                                        worlds[selected_world].seed,
                                    ) % 2
                                        == 0
                                {
                                    survival.harvest_berry();
                                    worlds[selected_world].berries = survival.berries;
                                }
                                worlds[selected_world].blocks = active_chunk.blocks.to_vec();
                                (mesh_batches, vertex_batches) =
                                    upload_chunk_mesh(&display, &active_chunk);
                                load_mesh_textures(&display, &mesh_batches, &mut block_textures);
                                if let Err(error) = persist_worlds(
                                    save_path.as_deref(),
                                    storage_write_enabled,
                                    &worlds,
                                ) {
                                    eprintln!("{error}");
                                    notice = Some("WORLD SAVE FAILED");
                                    storage_write_enabled = false;
                                }
                            }
                        }
                    }
                }
                glutin::event::WindowEvent::MouseInput {
                    state: glutin::event::ElementState::Pressed,
                    button: glutin::event::MouseButton::Right,
                    ..
                } if page == MenuPage::Playing => {
                    if let Some((_, adjacent)) = raycast_block(
                        &active_chunk,
                        camera_position,
                        Vector3::new(
                            camera_yaw.sin() * camera_pitch.cos(),
                            camera_pitch.sin(),
                            camera_yaw.cos() * camera_pitch.cos(),
                        ),
                        5.0,
                    ) {
                        let block_id = block_for_hotbar_slot(selected_slot);
                        if active_chunk.get(adjacent[0], adjacent[1], adjacent[2]) == AIR
                            && !block_intersects_player(adjacent, camera_position)
                            && active_chunk.set_if_in_bounds(
                                adjacent[0],
                                adjacent[1],
                                adjacent[2],
                                block_id,
                            )
                        {
                            worlds[selected_world].blocks = active_chunk.blocks.to_vec();
                            (mesh_batches, vertex_batches) =
                                upload_chunk_mesh(&display, &active_chunk);
                            load_mesh_textures(&display, &mesh_batches, &mut block_textures);
                            if let Err(error) =
                                persist_worlds(save_path.as_deref(), storage_write_enabled, &worlds)
                            {
                                eprintln!("{error}");
                                notice = Some("WORLD SAVE FAILED");
                                storage_write_enabled = false;
                            }
                        }
                    }
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
                        (MenuPage::Main, Some("SINGLEPLAYER")) => {
                            page = MenuPage::Worlds;
                            notice = None;
                        }
                        (MenuPage::Main, Some("MULTIPLAYER")) => {
                            notice = Some("MULTIPLAYER IS NOT READY");
                        }
                        (MenuPage::Main, Some("MODS & ADD-ONS")) => {
                            notice = Some("ADDONS ARE NOT READY");
                        }
                        (MenuPage::Main, Some("SETTINGS")) => {
                            settings_return_page = MenuPage::Main;
                            page = MenuPage::Settings;
                        }
                        (MenuPage::Main, Some("EXIT")) => {
                            *control_flow = glutin::event_loop::ControlFlow::Exit;
                        }
                        (MenuPage::Worlds, Some("PREVIOUS WORLD")) if !worlds.is_empty() => {
                            selected_world = (selected_world + worlds.len() - 1) % worlds.len();
                            world_style = worlds[selected_world].style;
                        }
                        (MenuPage::Worlds, Some("NEXT WORLD")) if !worlds.is_empty() => {
                            selected_world = (selected_world + 1) % worlds.len();
                            world_style = worlds[selected_world].style;
                        }
                        (MenuPage::Worlds, Some("CREATE WORLD"))
                            if worlds.len() < MAX_SAVED_WORLDS =>
                        {
                            let name = format!("WORLD {}", worlds.len() + 1);
                            worlds.push(WorldSave::new(name, world_style, next_world_seed));
                            next_world_seed = next_world_seed.wrapping_add(1);
                            selected_world = worlds.len() - 1;
                            if let Err(error) =
                                persist_worlds(save_path.as_deref(), storage_write_enabled, &worlds)
                            {
                                eprintln!("{error}");
                                notice = Some("WORLD SAVE FAILED");
                                storage_write_enabled = false;
                            }
                        }
                        (MenuPage::Worlds, Some("CREATE WORLD")) => {
                            notice = Some("WORLD LIMIT REACHED");
                        }
                        (MenuPage::Worlds, Some("PLAY WORLD")) if !worlds.is_empty() => {
                            game_mode = None;
                            world_style = worlds[selected_world].style;
                            page = MenuPage::Modes;
                        }
                        (MenuPage::Worlds, Some("DELETE WORLD")) if worlds.len() > 1 => {
                            worlds.remove(selected_world);
                            selected_world = selected_world.min(worlds.len() - 1);
                            world_style = worlds[selected_world].style;
                            if let Err(error) =
                                persist_worlds(save_path.as_deref(), storage_write_enabled, &worlds)
                            {
                                eprintln!("{error}");
                                notice = Some("WORLD SAVE FAILED");
                                storage_write_enabled = false;
                            }
                        }
                        (MenuPage::Worlds, Some("DELETE WORLD")) => {
                            notice = Some("KEEP ONE WORLD");
                        }
                        (MenuPage::Worlds, Some("BACK")) => page = MenuPage::Main,
                        (MenuPage::Modes, Some("SURVIVAL")) => {
                            game_mode = Some(GameMode::Survival);
                            active_chunk = worlds[selected_world]
                                .chunk()
                                .expect("loaded world block count is valid");
                            (mesh_batches, vertex_batches) =
                                upload_chunk_mesh(&display, &active_chunk);
                            load_mesh_textures(&display, &mesh_batches, &mut block_textures);
                            camera_position = Vector3::new(
                                8.5,
                                surface_height(&active_chunk, 8, 13) as f32 + PLAYER_EYE_HEIGHT,
                                13.5,
                            );
                            mobs = spawn_mobs(
                                worlds[selected_world].style,
                                worlds[selected_world].seed,
                                &active_chunk,
                            );
                            mob_vertex_buffer.write(&build_mob_vertices(&mobs));
                            mob_mesh_dirty = false;
                            survival = SurvivalState::new(worlds[selected_world].berries);
                            defeats = 0;
                            attack_cooldown = 0.0;
                            vertical_velocity = 0.0;
                            page = MenuPage::Playing;
                        }
                        (MenuPage::Modes, Some("CREATIVE")) => {
                            game_mode = Some(GameMode::Creative);
                            active_chunk = worlds[selected_world]
                                .chunk()
                                .expect("loaded world block count is valid");
                            (mesh_batches, vertex_batches) =
                                upload_chunk_mesh(&display, &active_chunk);
                            load_mesh_textures(&display, &mesh_batches, &mut block_textures);
                            camera_position = Vector3::new(
                                8.5,
                                surface_height(&active_chunk, 8, 13) as f32 + PLAYER_EYE_HEIGHT,
                                13.5,
                            );
                            mobs = spawn_mobs(
                                worlds[selected_world].style,
                                worlds[selected_world].seed,
                                &active_chunk,
                            );
                            mob_vertex_buffer.write(&build_mob_vertices(&mobs));
                            mob_mesh_dirty = false;
                            survival = SurvivalState::new(worlds[selected_world].berries);
                            defeats = 0;
                            attack_cooldown = 0.0;
                            vertical_velocity = 0.0;
                            page = MenuPage::Playing;
                        }
                        (MenuPage::Dead, Some("RESPAWN")) => {
                            active_chunk = worlds[selected_world]
                                .chunk()
                                .expect("loaded world block count is valid");
                            (mesh_batches, vertex_batches) =
                                upload_chunk_mesh(&display, &active_chunk);
                            camera_position = Vector3::new(
                                8.5,
                                surface_height(&active_chunk, 8, 13) as f32 + PLAYER_EYE_HEIGHT,
                                13.5,
                            );
                            survival = SurvivalState::new(worlds[selected_world].berries);
                            vertical_velocity = 0.0;
                            player_grounded = true;
                            mobs = spawn_mobs(
                                worlds[selected_world].style,
                                worlds[selected_world].seed,
                                &active_chunk,
                            );
                            mob_vertex_buffer.write(&build_mob_vertices(&mobs));
                            mob_mesh_dirty = false;
                            attack_cooldown = 0.0;
                            page = MenuPage::Playing;
                        }
                        (MenuPage::Dead, Some("MAIN MENU")) => {
                            game_mode = None;
                            page = MenuPage::Main;
                        }
                        (MenuPage::Modes, Some("BACK")) => page = MenuPage::Worlds,
                        (
                            MenuPage::Settings,
                            Some(
                                "WORLD HILLS" | "WORLD FLAT" | "WORLD FROSTLANDS"
                                | "WORLD FRONTIER",
                            ),
                        ) => {
                            world_style = match world_style {
                                WorldStyle::Hills => WorldStyle::Flat,
                                WorldStyle::Flat => WorldStyle::Frostlands,
                                WorldStyle::Frostlands => WorldStyle::Frontier,
                                WorldStyle::Frontier => WorldStyle::Hills,
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
                    let gl_window = display.gl_window();
                    let window = gl_window.window();
                    if let Err(error) = configure_cursor(window, page == MenuPage::Playing) {
                        eprintln!("{error}");
                        if page == MenuPage::Playing {
                            notice = Some("MOUSE CAPTURE UNAVAILABLE");
                        }
                    }
                }
                glutin::event::WindowEvent::MouseWheel { delta, .. }
                    if page == MenuPage::Playing =>
                {
                    let direction = match delta {
                        glutin::event::MouseScrollDelta::LineDelta(_, y) => y,
                        glutin::event::MouseScrollDelta::PixelDelta(position) => position.y as f32,
                    };
                    selected_slot = if direction > 0.0 {
                        (selected_slot + 8) % 9
                    } else if direction < 0.0 {
                        (selected_slot + 1) % 9
                    } else {
                        selected_slot
                    };
                }
                glutin::event::WindowEvent::KeyboardInput { input, .. } => {
                    if let Some(key) = input.virtual_keycode {
                        match input.state {
                            glutin::event::ElementState::Pressed => {
                                pressed_keys.insert(key);
                                if page == MenuPage::Playing {
                                    selected_slot = match key {
                                        glutin::event::VirtualKeyCode::Key1 => 0,
                                        glutin::event::VirtualKeyCode::Key2 => 1,
                                        glutin::event::VirtualKeyCode::Key3 => 2,
                                        glutin::event::VirtualKeyCode::Key4 => 3,
                                        glutin::event::VirtualKeyCode::Key5 => 4,
                                        glutin::event::VirtualKeyCode::Key6 => 5,
                                        glutin::event::VirtualKeyCode::Key7 => 6,
                                        glutin::event::VirtualKeyCode::Key8 => 7,
                                        glutin::event::VirtualKeyCode::Key9 => 8,
                                        _ => selected_slot,
                                    };
                                }
                                if key == glutin::event::VirtualKeyCode::F
                                    && page == MenuPage::Playing
                                    && game_mode == Some(GameMode::Survival)
                                {
                                    if survival.consume_berry() {
                                        worlds[selected_world].berries = survival.berries;
                                        notice = Some("ATE A SWEET BERRY");
                                        if let Err(error) = persist_worlds(
                                            save_path.as_deref(),
                                            storage_write_enabled,
                                            &worlds,
                                        ) {
                                            eprintln!("{error}");
                                            notice = Some("WORLD SAVE FAILED");
                                            storage_write_enabled = false;
                                        }
                                    } else {
                                        notice = Some(if survival.berries == 0 {
                                            "NO BERRIES: HARVEST LEAVES OR DEFEAT THREATS"
                                        } else {
                                            "HUNGER IS ALREADY FULL"
                                        });
                                    }
                                }
                                if key == glutin::event::VirtualKeyCode::Escape
                                    && notice.is_some()
                                    && page != MenuPage::Playing
                                {
                                    notice = None;
                                } else if key == glutin::event::VirtualKeyCode::Escape {
                                    page = match page {
                                        MenuPage::Playing => MenuPage::Pause,
                                        MenuPage::Pause => MenuPage::Playing,
                                        MenuPage::Settings => settings_return_page,
                                        MenuPage::Modes => MenuPage::Worlds,
                                        MenuPage::Worlds => MenuPage::Main,
                                        MenuPage::Main => MenuPage::Main,
                                        MenuPage::Dead => MenuPage::Main,
                                    };
                                    let gl_window = display.gl_window();
                                    let window = gl_window.window();
                                    if let Err(error) =
                                        configure_cursor(window, page == MenuPage::Playing)
                                    {
                                        eprintln!("{error}");
                                        if page == MenuPage::Playing {
                                            notice = Some("MOUSE CAPTURE UNAVAILABLE");
                                        }
                                    }
                                }
                                if key == glutin::event::VirtualKeyCode::Space
                                    && page == MenuPage::Playing
                                    && game_mode == Some(GameMode::Survival)
                                    && player_grounded
                                {
                                    vertical_velocity = 7.0;
                                    player_grounded = false;
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
                    let forward_amount = pressed_keys.contains(&glutin::event::VirtualKeyCode::W)
                        as u8 as f32
                        - pressed_keys.contains(&glutin::event::VirtualKeyCode::S) as u8 as f32;
                    let right_amount = pressed_keys.contains(&glutin::event::VirtualKeyCode::D)
                        as u8 as f32
                        - pressed_keys.contains(&glutin::event::VirtualKeyCode::A) as u8 as f32;
                    let mut horizontal_move = forward * forward_amount + right * right_amount;
                    let move_length = (horizontal_move.x * horizontal_move.x
                        + horizontal_move.z * horizontal_move.z)
                        .sqrt();
                    if move_length > 0.0 {
                        horizontal_move = horizontal_move / move_length * speed;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Left) {
                        camera_yaw -= delta;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Right) {
                        camera_yaw += delta;
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Up) {
                        camera_pitch = (camera_pitch + delta).min(1.45);
                    }
                    if pressed_keys.contains(&glutin::event::VirtualKeyCode::Down) {
                        camera_pitch = (camera_pitch - delta).max(-1.45);
                    }
                    let moved = move_player(
                        &active_chunk,
                        camera_position,
                        Vector3::new(horizontal_move.x, 0.0, horizontal_move.z),
                    );
                    camera_position.x = moved
                        .x
                        .clamp(PLAYER_RADIUS, CHUNK_WIDTH as f32 - PLAYER_RADIUS);
                    camera_position.z = moved
                        .z
                        .clamp(PLAYER_RADIUS, CHUNK_DEPTH as f32 - PLAYER_RADIUS);

                    if game_mode == Some(GameMode::Creative) {
                        let vertical_amount = pressed_keys
                            .contains(&glutin::event::VirtualKeyCode::Space)
                            as u8 as f32
                            - pressed_keys.contains(&glutin::event::VirtualKeyCode::LShift) as u8
                                as f32;
                        camera_position.y += vertical_amount * speed;
                    } else {
                        let grounded_below = player_collides(
                            &active_chunk,
                            camera_position - Vector3::new(0.0, 0.04, 0.0),
                        );
                        if !grounded_below {
                            vertical_velocity -= 20.0 * delta;
                        } else if vertical_velocity < 0.0 {
                            vertical_velocity = 0.0;
                        }
                        let before_y = camera_position.y;
                        camera_position = move_player(
                            &active_chunk,
                            camera_position,
                            Vector3::new(0.0, vertical_velocity * delta, 0.0),
                        );
                        if camera_position.y == before_y && vertical_velocity < 0.0 {
                            vertical_velocity = 0.0;
                        }
                        player_grounded = player_collides(
                            &active_chunk,
                            camera_position - Vector3::new(0.0, 0.04, 0.0),
                        );
                    }
                    attack_cooldown = (attack_cooldown - delta).max(0.0);
                    if game_mode == Some(GameMode::Survival) {
                        survival.tick(delta, move_length > 0.0);
                        let fall_damage = survival.update_fall(camera_position.y, player_grounded);
                        if fall_damage > 0 {
                            survival.damage(fall_damage);
                            notice = Some("HARD LANDING");
                        }
                        if update_mobs(
                            &mut mobs,
                            &active_chunk,
                            camera_position,
                            delta,
                            &mut survival,
                        ) {
                            mob_mesh_dirty = true;
                        }
                        if camera_position.y < -16.0 {
                            survival.health = 0;
                            notice = Some("LOST TO THE VOID");
                        }
                        if survival.health == 0 {
                            page = MenuPage::Dead;
                            pressed_keys.clear();
                            if let Err(error) =
                                configure_cursor(display.gl_window().window(), false)
                            {
                                eprintln!("{error}");
                                notice = Some("COULD NOT RELEASE MOUSE");
                            }
                        }
                    }
                }
                display.gl_window().window().request_redraw();
            }
            glutin::event::Event::RedrawRequested(_) => {
                let mut target = display.draw();
                let fog_color = match world_style {
                    WorldStyle::Frostlands => [0.54, 0.70, 0.86],
                    WorldStyle::Frontier => frontier_fog_color(camera_position.x),
                    WorldStyle::Hills | WorldStyle::Flat => [0.49, 0.67, 1.0],
                };
                target.clear_color_and_depth((fog_color[0], fog_color[1], fog_color[2], 1.0), 1.0);

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
                                block_texture: texture
                                    .sampled()
                                    .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
                                    .minify_filter(glium::uniforms::MinifySamplerFilter::Nearest),
                                camera_position: [
                                    camera_position.x,
                                    camera_position.y,
                                    camera_position.z,
                                ],
                                fog_color: fog_color,
                                fog_range: [12.0f32, 48.0f32],
                            },
                            &voxel_parameters,
                        )
                        .expect("draw chunk mesh");
                }
                if mob_mesh_dirty {
                    mob_vertex_buffer.write(&build_mob_vertices(&mobs));
                    mob_mesh_dirty = false;
                }
                let living_threats = mobs.iter().filter(|mob| mob.alive).count();
                if living_threats > 0 {
                    target
                        .draw(
                            &mob_vertex_buffer,
                            &indices,
                            &voxel_program,
                            &uniform! {
                                matrix: matrix,
                                block_texture: mob_texture
                                    .sampled()
                                    .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest),
                                camera_position: [
                                    camera_position.x,
                                    camera_position.y,
                                    camera_position.z,
                                ],
                                fog_color: fog_color,
                                fog_range: [12.0f32, 48.0f32],
                            },
                            &voxel_parameters,
                        )
                        .expect("draw world mobs");
                }

                let menu_needs_rebuild = menu_buffer
                    .as_ref()
                    .map(|cached| {
                        let current_biome = biome_label(world_style, camera_position.x);
                        cached.width != width
                            || cached.height != height
                            || cached.page != page
                            || cached.world_style != world_style
                            || cached.biome != current_biome
                            || cached.speed_index != speed_index
                            || cached.mode != game_mode
                            || cached.world_name
                                != worlds.get(selected_world).map(|world| world.name.clone())
                            || cached.world_count != worlds.len()
                            || cached.selected_index != selected_world
                            || cached.notice != notice
                            || cached.health != survival.health
                            || cached.food != survival.hunger
                            || cached.berries != survival.berries
                            || cached.threats != living_threats
                            || cached.defeats != defeats
                            || cached.selected_slot != selected_slot
                    })
                    .unwrap_or(true);
                if menu_needs_rebuild {
                    let selected_world_data = worlds.get(selected_world);
                    let current_biome = biome_label(world_style, camera_position.x);
                    let ui_vertices = build_menu_vertices(
                        width as f32,
                        height as f32,
                        page,
                        world_style,
                        speed_index,
                        game_mode,
                        selected_world_data.map(|world| world.name.as_str()),
                        worlds.len(),
                        notice,
                        survival.health,
                        survival.hunger,
                        selected_slot,
                        current_biome,
                        survival.berries,
                        living_threats,
                        defeats,
                    );
                    let buffer = glium::VertexBuffer::new(&display, &ui_vertices)
                        .expect("upload menu vertex buffer");
                    menu_buffer = Some(MenuUiBuffer {
                        width,
                        height,
                        page,
                        world_style,
                        biome: current_biome,
                        speed_index,
                        mode: game_mode,
                        world_name: selected_world_data.map(|world| world.name.clone()),
                        world_count: worlds.len(),
                        selected_index: selected_world,
                        notice,
                        health: survival.health,
                        food: survival.hunger,
                        berries: survival.berries,
                        threats: living_threats,
                        defeats,
                        selected_slot,
                        buffer,
                    });
                }
                if let Some(cached) = &menu_buffer {
                    let ui_parameters = glium::DrawParameters {
                        blend: glium::Blend::alpha_blending(),
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
        assert_eq!(texture_for_block(JUNGLE_LOG, Face::Side), "jungle_log.png");
        assert_eq!(
            texture_for_block(RED_SANDSTONE, Face::Side),
            "red_sandstone.png"
        );
        assert_eq!(
            texture_for_block(MAGENTA_CRYSTAL, Face::Top),
            "magenta_crystal.png"
        );
    }

    #[test]
    fn maaz_craft_menu_buttons_fit_and_respond_to_clicks() {
        let labels = menu_labels(MenuPage::Main, WorldStyle::Hills, 1);
        let buttons = menu_layout(854.0, 480.0, &labels);
        assert_eq!(buttons.len(), 5);
        assert_eq!(buttons[0].label, "SINGLEPLAYER");
        assert_eq!(buttons[1].label, "MULTIPLAYER");
        assert_eq!(buttons[2].label, "MODS & ADD-ONS");
        assert_eq!(buttons[3].label, "SETTINGS");
        assert_eq!(buttons[4].label, "EXIT");
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
            Some("SINGLEPLAYER")
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
        assert!(hills.blocks.contains(&OAK_PLANKS));
        assert!(hills.blocks.contains(&COBBLESTONE));
        assert!(hills.blocks.contains(&GLASS));
        assert!(hills.blocks.contains(&PATH_BLOCK));
    }

    #[test]
    fn frostlands_generate_a_crystalline_temple_and_snow_materials() {
        let frostlands = Chunk::generate(WorldStyle::Frostlands, 51);
        assert_ne!(
            terrain_height(2, 3, WorldStyle::Frostlands, 51),
            terrain_height(12, 9, WorldStyle::Frostlands, 51)
        );
        assert!(frostlands.blocks.contains(&SNOW_BLOCK));
        assert!(frostlands.blocks.contains(&SPRUCE_LOG));
        assert!(frostlands.blocks.contains(&SPRUCE_LEAVES));
        assert!(frostlands.blocks.contains(&ICE));
        assert!(frostlands.blocks.contains(&BLUE_ICE));
        assert!(frostlands.blocks.contains(&ICE_BRICKS));
        assert!(frostlands.blocks.contains(&CRYSTAL));
        assert_eq!(block_fallback_color(CRYSTAL), [0.53, 0.93, 1.0]);
    }

    #[test]
    fn world_save_round_trips_seed_style_and_edited_blocks() {
        let mut world = WorldSave::new("TEST WORLD".to_string(), WorldStyle::Hills, 4242);
        world.berries = 13;
        world.blocks[Chunk::index(3, 7, 5)] = REDSTONE_BLOCK;
        let encoded = encode_worlds(&[world]).expect("encode world save");
        let decoded = decode_worlds(&encoded).expect("decode world save");

        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "TEST WORLD");
        assert_eq!(decoded[0].style, WorldStyle::Hills);
        assert_eq!(decoded[0].seed, 4242);
        assert_eq!(decoded[0].berries, 13);
        assert_eq!(
            decoded[0].chunk().expect("restore blocks").get(3, 7, 5),
            REDSTONE_BLOCK
        );
    }

    #[test]
    fn legacy_world_saves_migrate_with_starter_berries() {
        let legacy = format!(
            "MAAZCRAFT_WORLDS_1\nOLD WORLD|hills|7|{}\n",
            "00".repeat(WORLD_BLOCK_COUNT)
        );
        let decoded = decode_worlds(&legacy).expect("migrate legacy save");
        assert_eq!(decoded[0].name, "OLD WORLD");
        assert_eq!(decoded[0].berries, 6);
        assert_eq!(decoded[0].blocks, vec![AIR; WORLD_BLOCK_COUNT]);
    }

    #[test]
    fn frontier_world_generates_three_biomes_and_original_landmarks() {
        let chunk = Chunk::generate(WorldStyle::Frontier, 2026);
        assert_eq!(frontier_biome(2), Biome::Jungle);
        assert_eq!(frontier_biome(7), Biome::RedDesert);
        assert_eq!(frontier_biome(13), Biome::FrostTundra);
        for (x, expected) in [(2, GRASS_BLOCK), (7, RED_SAND), (13, SNOW_BLOCK)] {
            let y = terrain_height(x, 0, WorldStyle::Frontier, 2026) - 1;
            assert_eq!(chunk.get(x as i32, y as i32, 0), expected);
        }
        for block in [
            JUNGLE_LOG,
            JUNGLE_LEAVES,
            RED_SANDSTONE,
            ICE,
            BLUE_ICE,
            MAGENTA_CRYSTAL,
        ] {
            assert!(
                chunk.blocks.contains(&block),
                "missing generated block {block}"
            );
        }
    }

    #[test]
    fn rift_shrine_builds_a_distinct_crystal_landmark() {
        let mut chunk = Chunk::new();
        build_rift_shrine(&mut chunk, 4, 5, 2026);
        assert_eq!(chunk.get(6, 2, 6), MAGENTA_CRYSTAL);
        assert_eq!(chunk.get(4, 1, 5), ICE_BRICKS);
        assert_eq!(chunk.get(8, 1, 5), ICE_BRICKS);
    }

    #[test]
    fn frostlands_world_style_survives_save_round_trip() {
        let world = WorldSave::new("FROSTLANDS".to_string(), WorldStyle::Frostlands, 51);
        let decoded = decode_worlds(&encode_worlds(&[world]).expect("encode frostlands"))
            .expect("decode frostlands");
        assert_eq!(decoded[0].style, WorldStyle::Frostlands);
        assert_eq!(
            decoded[0].chunk().expect("restore frostlands").blocks,
            Chunk::generate(WorldStyle::Frostlands, 51).blocks
        );
    }

    #[test]
    fn frontier_world_style_survives_save_round_trip() {
        let world = WorldSave::new("WILDBIOME".to_string(), WorldStyle::Frontier, 2026);
        let decoded = decode_worlds(&encode_worlds(&[world]).expect("encode frontier"))
            .expect("decode frontier");
        assert_eq!(decoded[0].style, WorldStyle::Frontier);
        assert_eq!(
            decoded[0].chunk().expect("restore frontier").blocks,
            Chunk::generate(WorldStyle::Frontier, 2026).blocks
        );
    }

    #[test]
    fn invalid_world_save_is_rejected_without_panicking() {
        assert!(decode_worlds("not-a-world-save\n").is_err());
        assert!(encode_worlds(&[WorldSave {
            name: "INVALID|NAME".to_string(),
            style: WorldStyle::Flat,
            seed: 1,
            berries: 6,
            blocks: vec![AIR; WORLD_BLOCK_COUNT],
        }])
        .is_err());
    }

    #[test]
    fn raycast_returns_hit_and_adjacent_placement_cell() {
        let mut chunk = Chunk::new();
        chunk.set(3, 2, 4, STONE);
        let (hit, place) = raycast_block(
            &chunk,
            Vector3::new(3.5, 2.5, 1.5),
            Vector3::new(0.0, 0.0, 1.0),
            5.0,
        )
        .expect("block is in view");
        assert_eq!(hit, [3, 2, 4]);
        assert_eq!(place, [3, 2, 3]);
    }

    #[test]
    fn player_collision_stops_at_solid_blocks() {
        let mut chunk = Chunk::new();
        chunk.set(1, 1, 0, STONE);
        let start = Vector3::new(0.5, 2.62, 0.5);
        assert!(!player_collides(&chunk, start));
        let moved = move_player(&chunk, start, Vector3::new(1.0, 0.0, 0.0));
        assert_eq!(moved.x, start.x);
    }

    #[test]
    fn hud_contains_hotbar_crosshair_and_status_indicators() {
        let hud = build_hud_vertices(854.0, 480.0, 20, 16, 3, None, "JUNGLE", 4, 2, 1);
        assert!(hud.len() > 500);
        assert!(hud.iter().any(|vertex| vertex.color == [0.92, 0.16, 0.19]));
        assert!(hud.iter().any(|vertex| vertex.color == [0.93, 0.83, 0.47]));
        assert!(hud.iter().any(|vertex| vertex.color == [0.24, 0.75, 0.36]));
    }

    #[test]
    fn world_selector_exposes_create_play_delete_and_navigation_actions() {
        assert_eq!(
            menu_labels(MenuPage::Worlds, WorldStyle::Hills, 1),
            [
                "PREVIOUS WORLD",
                "NEXT WORLD",
                "CREATE WORLD",
                "PLAY WORLD",
                "DELETE WORLD",
                "BACK"
            ]
        );
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
        assert_eq!(
            menu_labels(MenuPage::Settings, WorldStyle::Frostlands, 1),
            ["WORLD FROSTLANDS", "SPEED NORMAL", "BACK"]
        );
        assert_eq!(
            menu_labels(MenuPage::Settings, WorldStyle::Frontier, 1),
            ["WORLD FRONTIER", "SPEED NORMAL", "BACK"]
        );
        assert_eq!(biome_label(WorldStyle::Frontier, 2.5), "JUNGLE");
        assert_eq!(biome_label(WorldStyle::Frontier, 7.5), "RED SAND DESERT");
        assert_eq!(biome_label(WorldStyle::Frontier, 13.5), "FROZEN TUNDRA");
        assert_ne!(frontier_fog_color(2.0), frontier_fog_color(7.0));
        assert_ne!(frontier_fog_color(7.0), frontier_fog_color(13.0));
    }

    #[test]
    fn redstone_has_a_distinct_particle_color() {
        assert_eq!(block_fallback_color(REDSTONE_BLOCK), [0.85, 0.08, 0.12]);
        assert_ne!(redstone_particle_color(), block_fallback_color(STONE));
    }

    #[test]
    fn missing_assets_get_repeatable_pixel_textures() {
        let snow = fallback_texture_data(SNOW_BLOCK);
        assert_eq!(snow.len(), 16 * 16 * 4);
        assert!(snow.iter().skip(3).step_by(4).all(|alpha| *alpha == 255));
        assert!(snow.chunks_exact(4).any(|pixel| pixel != &snow[..4]));
        assert_eq!(snow, fallback_texture_data(SNOW_BLOCK));
        assert_ne!(snow, fallback_texture_data(BLUE_ICE));
    }

    #[test]
    fn survival_food_healing_and_starvation_are_consistent() {
        let mut survival = SurvivalState::new(2);
        survival.damage(4);
        survival.tick(5.0, false);
        assert_eq!(survival.health, 17);
        assert_eq!(survival.hunger, 19);
        assert!(survival.consume_berry());
        assert_eq!(survival.berries, 1);
        assert_eq!(survival.hunger, 20);
        assert!(!survival.consume_berry());

        survival.hunger = 0;
        survival.tick(12.0, false);
        assert_eq!(survival.health, 15);
        assert!(!survival.consume_berry() || survival.hunger == 5);
    }

    #[test]
    fn fall_damage_starts_after_safe_drop_distance() {
        let mut survival = SurvivalState::new(0);
        assert_eq!(survival.update_fall(20.0, false), 0);
        assert_eq!(survival.update_fall(16.0, true), 2);
        survival.damage(2);
        assert_eq!(survival.health, 18);

        assert_eq!(survival.update_fall(12.0, false), 0);
        assert_eq!(survival.update_fall(9.0, true), 0);
        assert_eq!(survival.health, 18);
    }

    #[test]
    fn mobs_spawn_deterministically_move_and_attack() {
        let chunk = Chunk::generate(WorldStyle::Frontier, 99);
        let mut mobs = spawn_mobs(WorldStyle::Frontier, 99, &chunk);
        assert_eq!(mobs.len(), MAX_MOBS);
        assert!(mobs.iter().all(|mob| mob.alive && mob.health == 8));
        assert_eq!(
            mobs.iter().map(|mob| mob.position).collect::<Vec<_>>(),
            spawn_mobs(WorldStyle::Frontier, 99, &chunk)
                .iter()
                .map(|mob| mob.position)
                .collect::<Vec<_>>()
        );
        assert!(mobs.iter().any(|mob| mob.kind == MobKind::JungleStalker));
        assert!(mobs.iter().any(|mob| mob.kind == MobKind::DuneRaider));
        assert!(mobs.iter().any(|mob| mob.kind == MobKind::FrostWisp));

        let target = mobs[0].position + Vector3::new(0.0, 0.7, 0.0);
        let direction = Vector3::new(0.0, 0.0, 1.0);
        let origin = target - direction * 2.0;
        assert_eq!(ray_hits_mob(&chunk, origin, direction, &mobs, 4.0), Some(0));
        let mut blocked_chunk = Chunk::new();
        blocked_chunk.set(
            mobs[0].position.x.floor() as usize,
            mobs[0].position.y.floor() as usize,
            origin.z.floor() as usize + 1,
            STONE,
        );
        assert_eq!(
            ray_hits_mob(&blocked_chunk, origin, direction, &mobs, 4.0),
            None
        );
        let geometry = build_mob_vertices(&mobs);
        assert_eq!(geometry.len(), MAX_MOBS * MOB_VERTEX_COUNT);

        let mut survival = SurvivalState::new(0);
        let before = mobs[0].position;
        assert!(update_mobs(
            &mut mobs[..1],
            &chunk,
            before + Vector3::new(1.0, 1.62, 0.0),
            0.25,
            &mut survival
        ));
        assert_eq!(survival.health, 18);
    }
}
