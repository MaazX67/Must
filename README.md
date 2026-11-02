# Maaz Craft

An original Rust voxel-game prototype targeting legacy OpenGL 2.1.

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

The launch screen has original Maaz Craft branding. **Play Solo** opens a
Survival/Creative mode selector; **Settings** switches between generated hills
and flat terrain and cycles movement speed. A selected world is generated as a
16x128x16 voxel chunk with grass, dirt, stone, and simple trees. Chunk geometry
omits faces hidden by non-air neighbors. **Quit** closes the game.

In the world, use **W/A/S/D** to move, **Space/Shift** to move vertically, and
the arrow keys to look around. **Escape** opens or closes the pause menu.
Creative mode currently moves faster; survival rules, inventory, mining,
placing blocks, mobs, and larger structures are not implemented yet.

Block textures are loaded independently from `assets/` using filenames such
as `stone.png`, `dirt.png`, `grass_top.png`, `grass_side.png`, `oak_log.png`,
and `oak_leaves.png`.
Missing textures use solid block-color fallbacks. The supplied menu screenshot
is a visual reference only and is not included in the running game.

The chunk recognizes air (0), stone (1), grass (2), dirt (3), and redstone
block (152). Unit tests cover block storage, face culling, asset names, menu
layout and input, and the redstone fallback color.

The current renderer explicitly requests OpenGL 2.1. A Windows executable is
platform-specific; DirectX 10 is not currently supported by this OpenGL-based
renderer.