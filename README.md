# MaazCraft 2.0

An original Rust voxel-game release candidate targeting legacy OpenGL 2.1.

## Run

Install Rust and the system OpenGL/X11 development libraries required by Glium, then run:

```sh
cargo run
```

On Windows, install Rust and build with:

```powershell
cargo build --release
```

The executable is written to `target/release/MaazCraft.exe`. This build uses
OpenGL 2.1 through the Windows OpenGL system library; it is not a DirectX 10
renderer. The Windows binary was cross-compiled here but has not been run on a
Windows machine.

The original MaazCraft 2.0 menu uses a translucent dark left-side button
column, centered block-letter title, and corner version/developer labels over
the generated village scene. It offers Singleplayer, Multiplayer, Mods &
Add-ons, Settings, and Exit. Multiplayer and Mods & Add-ons currently show
not-ready notices. Singleplayer opens a world selector where you can cycle,
create, play, or delete worlds.
Fresh installations start with the Frontier showcase world. The Settings
screen cycles world styles; choose **WORLD FRONTIER** before creating another
showcase world.

Worlds generate a seeded 16x128x16 chunk. Hills and flat worlds have a small
wood-and-stone village; Frostlands worlds add snowy terrain, spruce trees, and
an original crystalline ice temple. The new Frontier world combines a jungle,
red-sand desert, and frozen tundra with transition strips, jungle trees, a
woodland hut, stepped red-sandstone pyramid, ice spikes, and a crystal Rift
Shrine. Jungle, red-sand, snow, ice, and crystal blocks use individual
textures when supplied and repeatable 16x16 procedural pixel textures
otherwise. The renderer adds directional face shading and biome-tinted
distance fog while retaining OpenGL 2.1 support; these low-cost effects are
not a substitute for an advanced commercial rendering pipeline.

The original in-game HUD includes a crosshair, health and hunger indicators,
berry supply, biome label, threat count, defeat count, and a pixel-styled
nine-slot block bar. Three themed enemy types patrol toward the player, attack
at close range, show health bars, and can be defeated with repeated left
clicks; attacks do not pass through solid blocks. Survival adds timed hunger,
food-based regeneration, starvation damage, fall damage, and a respawn screen.
Berries are harvested from some leaf blocks and defeated enemies, eaten with
**F**, and saved with their world. World edits persist using a temporary file
and backup so an interrupted save can be recovered. Existing version-1 saves
are migrated on load. Chunk geometry omits faces hidden by non-air neighbors.

In the world, use **W/A/S/D** to move, the mouse or arrow keys to look around,
and **Escape** to pause. Survival has jumping, collision, hunger, health
regeneration, starvation, fall damage, and a death/respawn flow. Eat berries
with **F**; harvest leaves or defeat enemies to find more. Three biome-themed
hostiles pursue and attack in Survival. Left-click attacks a targeted enemy or
mines the block behind it; right-click places the selected block. Use **1–9**
or the mouse wheel to select a hotbar slot. Creative mode retains free vertical
flight. World edits and berry supplies are saved immediately.

This is a 2.0 release candidate, not feature parity with large commercial
sandbox games: it still renders a single 16x128x16 chunk and has no chunk
streaming, crafting, or network multiplayer. Mobs respawn when a world is
entered again; combat stats are session-only.

Block textures are loaded independently from `assets/` using filenames such
as `stone.png`, `dirt.png`, `grass_top.png`, `grass_side.png`, `oak_log.png`,
`oak_leaves.png`, `oak_planks.png`, `cobblestone.png`, `glass.png`, `path.png`,
`snow_block.png`, `ice.png`, `packed_ice.png`, `blue_ice.png`, `spruce_log.png`,
`spruce_leaves.png`, `ice_bricks.png`, `crystal.png`, `jungle_log.png`,
`jungle_leaves.png`, `red_sand.png`, `red_sandstone.png`, and
`magenta_crystal.png`. Missing textures use procedural pixel-texture
fallbacks. Reference images in the workspace are not loaded or included by
the game.

The chunk recognizes air (0), stone (1), grass (2), dirt (3), jungle,
red-sand, snow, ice, spruce, and crystal materials, and redstone block (152).
Unit tests cover block storage, face culling, asset names, menu layout and
input, seeded village, Frostlands and multi-biome Frontier generation, save
round trips, HUD geometry, and block fallback colors.

The current renderer explicitly requests OpenGL 2.1. A Windows executable is
platform-specific; DirectX 10 is not currently supported by this OpenGL-based
renderer.