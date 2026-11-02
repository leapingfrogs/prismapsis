# Prismapsis — core design

Living document. Captures the core mechanics agreed so far; tuning values are
starting points to be adjusted in play.

## Pillars

1. **Bounce control.** Movement is mastered, not given: the hero bounces
   endlessly and the player steers by setting spin.
2. **Light is the resource.** Defeated enemies release light; the orbiting
   prism absorbs it; mixing light reshapes the world.
3. **Colour opens the world.** Restoring colours makes matching platforms,
   doors and routes real, so levels unfold as the spectrum is rebuilt.

## Hero movement

- Gravity pulls the hero down; every bounce returns to the **same fixed apex
  height**. No energy is lost on any bounce.
- **Spin** is set in **fixed steps from -5 to +5**. Step 0 bounces in place.
- On each **floor contact**, the current spin step sets horizontal speed
  (`speed = step × speed_per_step`). Horizontal speed is kept while airborne.
- Spin is **not** changed by friction or by hitting things (for now). Spin
  decay or transfer may come later as an advanced mechanic.
- **Walls** reverse horizontal movement **and flip the spin step** (e.g. +3
  becomes -3), so the hero carries on bouncing away from the wall. Chosen
  in playtesting over keeping the spin, which sent the hero straight back
  into the wall on the next floor bounce.
- **Ceilings** (e.g. a low platform overhead) cut the bounce short: the hero
  rebounds downward without losing energy, so the apex is capped by the
  ceiling.

## The prism companion

- A prism **orbits the hero automatically** at a steady speed.
- **Holding a button freezes it** in place on its orbit, e.g. to park it in
  front as a shield or to line it up with incoming light.
- The prism **absorbs light** it sweeps through.
- Controls: hold **Shift** (gamepad **left trigger**) to freeze. It
  brightens while frozen, and its tip always points away from the hero.
- While frozen, **Q** (gamepad **West**) nudges it anticlockwise and
  **E** (gamepad **East**) clockwise, slowly, for precise positioning in
  puzzles. The nudge keys do nothing while it orbits. (Nudging moved off
  up/down so those keys can steer the hero with anti-gravity.)
- It currently **passes through solids**; whether walls should block it is
  open until light and shooting are in.
- Starting tuning: orbit radius 90, 0.6 revolutions per second
  anticlockwise, nudge 0.25 revolutions per second.

## Shooting

- The player **controls when to fire**: holding **Space** (gamepad **South**
  or **right trigger**) keeps a continuous laser beam on. Chosen over
  automatic pulses so charging and firing become a strategic choice.
- **Beam energy** limits firing (starting values, all tunable):
  - 100 energy; firing drains 35/s, so a full bar lasts about 3 s.
  - Recharges at 25/s after a 0.6 s pause once firing stops.
  - Starting to fire needs at least 15 energy.
  - Running dry cuts the beam; fire must be released and pressed again.
  - An energy bar sits top-left and turns red when there isn't enough
    energy to start firing.
- It fires **horizontally in the direction of travel**: the way the hero is
  moving, or the way it last moved when spin is 0.
- The beam is a ray from the hero that **reaches the first wall**. Whatever
  it touches takes damage ticks (10 per second).
- A beam that crosses the prism **splits into red, green and blue beams**
  that fan out from the prism, centred on the direction pointing away from
  the hero. Moving the prism around its orbit aims the spread, which gives
  freezing and nudging a combat role. Split beams also reach the first wall
  and don't re-enter the prism.
- Because shots are horizontal, the prism only sits in the line of fire
  near the front or back of its orbit.
- Beam colour is **cosmetic**: all beams deal the same damage, and an
  enemy's own colour decides the light it releases (see below).
- Power-ups map onto the beam: rear shot adds a backwards beam, spread
  fans each beam into three, rapid fire speeds up damage ticks and
  recharging, and piercing lets beams pass through enemies.

## Light and the spectrum

- **Enemies have a colour**, red, green or blue, and release light of that
  colour **automatically when defeated**. There is no collection step. Each
  mote wave is a single colour, cycling red, green, blue. Placed lenses
  have a colour set by the level.
- Light fills **one gauge per primary colour**: a mote gives 1 unit and a
  lens 4, and a gauge holds 10, so roughly 10 kills restore a colour.
  These are tunable in the dev panel. A full gauge **restores** that colour.
- Light mixes **additively**, like real light: red + green = yellow,
  green + blue = cyan, red + blue = magenta, all three = white.
  **Secondaries restore once both of their primaries are restored**, and
  white once all three are. The order you restore colours is therefore a
  choice, and secondaries give levels mid-progress gates.
- Restored colours are **per level**: each level starts grey and has its own
  spectrum to restore.
- **Restoring white opens the level's exit.**
- **Dying keeps restored colours but halves any gauge still filling.**
- The beam's own colour stays **cosmetic**: an enemy's colour decides the
  light it gives, whichever beam defeats it.

### What restored colours do

- **Phantom platforms turn solid.** Before their colour is restored they
  are faint outlines the hero passes through; afterwards they are ledges to
  bounce on, opening higher routes. They can be primary or secondary
  coloured.
- **Hidden things appear.** Pickups, routes or enemies drawn in a colour
  are invisible until it is restored.
- **The world floods with colour.** The level starts greyscale. Each
  restored colour washes its shades into the background and scenery, with a
  visible flood effect, until with all three the level is revealed in full
  colour.

### Sound

- Light from a defeat plays its colour's note. Red, green and blue are the
  root, third and fifth, matching the split beam's chord.
- Restoring a colour plays a rising flourish, a secondary plays its two
  notes together, and restoring white plays the full triad as the exit
  opens.

## Enemies

- Enemies come in **distinct types**, each with a readable pattern;
  learning them is part of mastery.
- **Toughness varies by type**: weak enemies pop with a quick flick of the
  beam, tough ones need a sustained burst, so energy management matters.
- **Appearance:** some enemies are **placed in the level**; others arrive
  in **waves from the screen edges on a timer** while the hero is in a
  level section.
- **Threat:** touching any enemy costs a life. **Some types shoot** slow
  projectiles at the hero.
- **The prism always blocks enemy shots**, so its orbit works as a rotating
  shield and freezing it in place covers one side. (The mirror-face
  power-up upgrades this from blocking to reflecting.)
- Enemy shots are stopped by solids.
- **On defeat**, enemies burst and may leave a power-up (see below). Light
  arrives in iteration 5.

### First enemy types

| Type | Appears | Behaviour | Toughness |
|------|---------|-----------|-----------|
| **Mote** | Waves from the edges | Small sparks drifting across in sine-wave formations; dangerous in numbers. | Weak: a few beam ticks. |
| **Lens** | Placed in the level | Fixed crystal turret that fires slow orbs of light at the hero. | Tough: needs a sustained burst. |

## Power-ups

- Shooting an enemy has a **chance to drop a power-up**.
- The drop stays **fixed where the enemy died** for a limited time, then
  vanishes (blinking as a warning near the end). Reaching it means planning
  spin and bounces.
- **Only the hero** can collect power-ups (not the prism).
- Collected abilities are **kept until the hero dies**, then all are lost.
  They carry on into the next level when the hero finishes one, as only
  losing a life takes them away.
- Each drop **shows which power-up it holds**, so players can decide
  whether it is worth the detour.
- Drops **favour abilities the hero doesn't have yet**, so a fresh life
  rebuilds quickly; once everything is collected, drops fall back to
  any power-up at random.

### Abilities

| Ability | Glyph | Effect |
|---------|-------|--------|
| Anti-gravity | Arrow over a line (violet) | Gravity stops and bouncing ends: **Up/W and Down/S** (d-pad) move the hero directly, while horizontal speed still comes from the spin steps. Floors and ceilings stop it instead of bouncing it. For narrow spaces and precise positioning. |
| Rear shot | Double arrow (orange) | A second beam fires backwards. |
| Spread fire | Three rays (green) | Each beam fans into three, 12° apart. |
| Rapid fire | Fast-forward (yellow) | Damage ticks and recharge are 1.6× faster. |
| Piercing | Line through a ring (blue) | Beams pass through enemies, hitting everything up to the next wall or the prism. |
| Mirror face | *Later* | Upgrades the prism from blocking enemy shots to **reflecting** them back at enemies. |
| Light nova | *Later* | A burst that clears nearby enemies, usable again after a recharge (e.g. 30 s). Needs its own button. |

Weapon power-ups are **separate pickups that stack**: rear shot, spread,
rapid fire and piercing all apply together once collected. With rear shot
and spread together the hero fires six beams, each of which can split
through the prism. A beam touching an enemy several times still deals one
tick.

Pickups are a coloured ring around a white glyph. They pulse gently and
blink faster during their last 2 s. Collected abilities show as coloured
bars under the life pips.

### Beam energy as a power-up lever

Beam energy gives power-ups a cost dimension as well as a benefit:

- **Capacity and recharge upgrades**, such as a larger energy bar for
  longer shots, faster recharge, or a shorter recharge delay.
- **Energy-hungry abilities**, where stronger weapons cost more to use.
  For example, rear shot or piercing could raise the drain rate while
  active, so collecting more power trades against how long you can fire.
- **Decided in iteration 4:** collected abilities are always on and cost no
  extra energy. Balance comes from drop rates for now; energy costs can be
  added later if stacked weapons feel too strong.

## Levels

- **Wide horizontal levels, scrolling both ways.** Negative spin travels
  left, so backtracking is natural.
- Progress means restoring a level's spectrum to open its exit.
- **One room per level** with a seamless scrolling camera. Most levels are
  one screen tall (1080 units) and any width. Some are taller, so the camera
  follows on both axes.
- **Fixed sequence.** Reaching the exit loads the next level, and the last
  leads back to the first. The order is listed in
  `assets/levels/campaign.toml`.
  Playing one file with `--level` repeats it.
- A level can override wave and light settings. Settings it leaves out go
  back to their defaults, so one level's settings don't carry into the next.
- **Checkpoints.** Touching a checkpoint tile moves the respawn point. A
  room reset (losing every life, or finishing) starts again from the start.
- Mote waves appear at heights measured from the bottom of the view, so
  they reach the hero in tall levels too.

### Level format

Each level is a TOML file whose layout is drawn as an ASCII grid, so it
reads well in a terminal or text editor and is easy to generate.

- `tile` sets the size of one character in world units (40 by default, so
  a 1080-unit level is 27 rows).
- A legend entry can set a `height` thinner than a tile for slim platforms.
  The platform keeps its top edge on the tile's top edge.
- `map` is a multi-line literal string holding the grid. An optional
  `background` grid of the same size holds non-colliding scenery.
- `[legend]` maps a character to what it places: a tile kind with a hue and
  optional traits, an entity, or a marker. A built-in default legend covers
  common cases, so many levels need no legend at all:
  - `#` is a grey wall, and `R G B Y C M W` are walls of that hue.
  - `r g b y c m w` are phantom platforms of that hue.
  - `=` is a one-way platform, `^` a spring pad, `~` mud and `%` a mirror
    block, all grey; see "Tile kinds".
  - `@` is the start, `X` the exit, `K` a checkpoint, and `.` or a space
    is empty.

  A level's legend adds or overrides characters, so it can use any type and
  hue combination without a fixed character for every pairing.
- Runs of the same character merge into rectangles on load. Collision and
  colour shading therefore work on a few large shapes, not on every tile.
- Entities such as lenses, pickups and spawners sit at their tile's centre.
- Ordinary TOML fields hold settings that aren't geometry: global enemy
  waves and their colours, and per-level light tuning overrides. Spawner
  tiles can add local enemies.
- Level files live in `assets/levels`. The test room is built into the game.
  `just run --level <file>` plays another file, and dev builds reload the
  level being played whenever its file is saved.

### Tile kinds

- **Wall:** solid, shaded by its hue.
- **Phantom:** becomes solid once its hue is restored.
- **One-way platform:** the hero passes up through it and lands on top.
- **Moving platform:** its path is drawn in the grid as a track character,
  and the legend sets its speed. The hero rides with it:
  - When bouncing, the platform's velocity is added to the bounce, so the
    platform's movement skews it.
  - With anti-gravity, a hero resting on the platform is carried along, on
    top of their own horizontal movement.
- **Crumbling:** gives way after being landed on a set number of times.
- **Breakable:** only the matching split beam colour breaks it, so a red
  block needs the red beam.
- **Traits** set in the legend, on walls and phantoms. They combine, so a
  one-way phantom platform or a springy mirror is one legend entry:
  - `one_way = true`: only its top is solid. The hero rises through it,
    passes it sideways and lands on it from above. Hovering with
    anti-gravity also rises through it and rests on top.
  - `bounce = <multiplier>`: scales the height of bounces off its top,
    from 0.25 to 3. Above 1 is a spring pad (the default `^` is 1.6), below
    1 is mud (the default `~` is 0.5).
  - `mirror = true`: every face reflects the beam by its angle of
    incidence, up to four times per beam. Split colour beams reflect too,
    and a reflected white beam still splits at the prism.
- The default `=` is 10 units thick and `^` 20, with their tops on the
  tile's top; `~` and `%` fill the tile.

### Level tooling

- `just level-show [file]...` prints levels with ANSI colours per hue:
  every level by default, and `campaign.toml` shows its levels in play
  order (`level-check` treats it the same way).
- `just level-check` reports every problem in the level files, and a test
  runs the same checks in CI. It checks that every character is in the
  legend, that the level has one start and an exit, and that legend entries
  and settings make sense. Tracks will be checked once moving platforms
  exist.
- `just level-check` also proves each level can be finished; see "Level
  generation".
- `just levelgen` writes the generated levels from their seeds.

### Level generation

Generated levels make up the campaign. The generator runs offline and its
output is committed; the game only ever loads level files.

- **Seeded and reproducible.** A generated level file keeps the settings it
  was made from in a `[generated]` table: seed, shape (`wide` or `tall`),
  gate colours, lenses per gate, tile features and wave interval. To add a
  level, write a file with just its `name` and `[generated]` table, then run
  `just levelgen`, which fills in the rest. Generated files are never edited
  by hand: change the settings and regenerate instead.
- **Every layout is checked before it's written.** It must be finishable
  by a weaker hero (three quarters of the bounce, spin up to 4), so no
  required jump is near the edge of the real hero's reach. Every checkpoint
  must be in reach. Taking away any gate's phantoms must make the level
  unfinishable even for the full-strength hero, so no gate can be bypassed.
  A layout that fails is thrown away and the next attempt tried, in an
  order fixed by the seed. The file records the attempt that passed.
- **Up to date.** `just levelgen --check` and a test lay every generated
  level out again from its recorded attempt and fail if a file differs.
  Layout is deterministic, so a matching file is the level that passed its
  checks.
- **Separate random numbers per stage.** Layout, terrain, ledges, lenses,
  pickups, decoration and features each draw from their own stream, so
  changing how one is made leaves the others as they were.
- **`assets/levels/campaign.toml`** lists the levels in play order. The
  test room, the tower and the workshop (every tile kind in one room) are
  dev levels and test fixtures, played with `--level`.
- **Wide levels** run over ground built from short patterns that give it
  rhythm: flat stretches, stairs, hops between pillars, pits that widen,
  and alternating steps, none stepping more than two tiles. Each gate is a
  wall of its colour too high to bounce over, with two phantom steps of
  that colour before it.
- **Tall levels** climb platforms on alternating sides, five tiles apart.
  Each gate is a phantom platform the climb can't skip.
- **Tile features** are opt-in: a recipe's `features` lists any of
  `platforms`, `springs`, `mud` and `mirrors`, and a layout without each
  one asked for is thrown away like an unsound one. They draw from their
  own random stream, and only plain walls change, so a recipe without
  features lays out exactly as before.
  - Wide levels: plain ledges become one-way platforms or mirrors; mud
    floors some pits and patches of flat ground; pillars become mirrors; a
    spring pad sits on a flat stretch of each segment. The start,
    checkpoints and gate approaches keep plain ground, and springs stay
    well clear of gates.
  - Tall levels: plain platforms become one-way platforms or mirrors; a
    spring pad sits at the wall end of some platforms, never within two
    platforms of a gate, so a spring can't skip one; mirror patches line
    the side walls. Tall levels have no mud, which would stall the climb.
  - The campaign introduces them in turn: platforms from level 2, springs
    from 3, mud from 4, mirrors from 5, and all four in the last two.
- **Difficulty rises** with more colour gates, more enemy pressure (shorter
  wave intervals, more lenses) and longer levels. Jumps stay comfortable.
- **Colours return primaries first.** Early levels gate on red, green and
  blue; later ones on yellow, cyan and magenta, each needing its two
  primaries. Lenses of a gate's colour sit before it, a checkpoint follows
  it, and a pickup hidden until the gate's colour returns sits on a ledge
  nearby. Anti-gravity is never placed, as it would float past the gates.
- **A solvability checker** proves a level can be finished. It simulates
  each bounce with the game's own physics: from every place the hero can
  land, with every spin, to where it lands next. It searches those landings
  together with which primaries are restored, since restoring a colour makes
  its phantoms solid. A primary counts as restorable when the level's waves
  include it, or when enough lens light is in the beam's reach. Picking up
  anti-gravity lets the hero hover anywhere open. One-way platforms and
  springs and mud need no rules of their own, as the physics handles them;
  each landing launches the next bounce at its surface's speed, and
  hovering can't move down into a one-way platform from above. Mirrors
  count as ordinary walls for the beam, so the reach of lenses is never
  overstated. The level is solvable if
  the exit can be touched with white restored. `just level-check` runs it
  on every level, hand-made ones included, and says whether each is
  comfortable.
- **First campaign:** eight levels, six wide and two tall.
- **Later:** branches and parallel routes (optional side paths to pickups,
  two ways past a gate), hand-drawn set pieces spliced into the route, and
  placing the changing tile kinds (6b-2) once they exist; moving, crumbling
  and breakable platforms will each bring a reach rule of their own.
- **Background reading:** Compton and Mateas, "Procedural Level Design for
  Platform Games" (AAAI, 2006), for rhythm patterns and measuring jump
  difficulty from the physics; Fisher, "How to make insane procedural
  platformer levels" (Game Developer, 2012), for validating with a player
  AI.

## Damage

- **One hit loses a life** and returns the hero to the last checkpoint.

## Pausing

- **P** (gamepad **Start**) pauses and resumes. The game clock stops, so
  the simulation, timers, animations and sounds all hold still, and a
  "Paused" overlay shows.
- Play inputs are ignored while paused, so pressing spin or fire then
  changes nothing on resume.
- Level files still reload while paused, so a level can be edited without
  the hero moving.

## Screens and score

- **The title** shows over the campaign's first level, greyed and still,
  with Play, Settings and Quit. Playing starts a new game from the first level with
  no abilities and no points. A level given with `--level` goes straight
  into play.
- **Game over** comes when the last life is lost. It shows the score and
  the best, and offers Continue, which plays the same level again with the
  score back at nothing, or going back to the title.
- **Finishing the campaign** shows the score and the best, then goes back
  to the title.
- **Settings**, from the title, set the master, music and effects volumes
  in steps of 10% with left and right. Changing the master or effects
  volume plays a chime to hear it by; the music plays throughout. They're
  saved on going back, to `prismapsis/settings.toml` in the user's
  configuration directory, and loaded at start.
- **Menus** stop the game clock as pausing does, so the level waits behind
  them, and P does nothing. Up and down move through a menu; enter, space
  or the gamepad's south or start button chooses. A menu ignores presses
  for its first 0.6 s, so a press meant for play doesn't choose something.
- **Score**, counted from the game's events: 100 for a mote, 500 for a
  lens, 250 for a pickup, 1000 for each colour restored (white included)
  and 5000 for finishing a level. It shows in the top right corner. The
  best score lasts until the game is closed; it isn't saved yet.

## Web build

- **Played from the project's GitHub Pages site.** Each release builds the
  web version, publishes it there and attaches it to the release as a zip.
  CI checks every change still builds for the web.
- **WebGL2**, which every current browser has, Safari on iOS included.
  WebGPU can come later as a second build that the page picks when the
  browser has it.
- **Small enough to download:** Bevy's 3D renderer and Ogg support are left
  out of every build, as this is a 2D game; the `web` profile optimises for
  size with full link-time optimisation, and `wasm-opt` shrinks it further,
  using only the WebAssembly features the Rust target enables by default.
  The result is about 26 MB, 11 MB compressed, of which about 5.6 MB is the
  embedded art and audio.
- **The page** (`web/index.html`) gives the game a `#game` canvas filling
  the window; keys like space don't scroll it. Browsers keep audio
  suspended until the player interacts, so the page resumes it on the first
  key, click or touch. It already stops scrolling, zooming and the
  long-press menu, ready for touch controls (8c).
- **In a browser** the title has no Quit, the clock comes from `web-time`,
  and settings aren't saved yet, as there's no configuration directory;
  the browser's local storage could keep them later.
- `just web` builds it into `dist/web`; `just web-serve` serves it locally.

## Resolution and art

- The world is **1080 units tall** at any window size; width follows the
  aspect ratio.
- At 4K each world unit covers 2×2 pixels, so sprite art should be authored
  at **2× its in-world size** to stay sharp at 4K: an 80 px tile for a
  40-unit cell.

### Generated art

- **All art is generated offline and committed**, like the levels and sound
  effects: the `art` tool draws it into `assets/art/`, `just art` regenerates
  it, and an up-to-date check fails when a committed image differs. Generated
  images are never hand-edited.
- **Walls use edge and corner tiles chosen on a dual grid.** Tiles are drawn
  on a grid offset half a cell from the level grid; each looks at the four
  cells it overlaps, wall or empty, and picks one of 16 tiles. All 16 are
  drawn, rather than rotating a few, because light comes from above: tops
  are lit and undersides shaded. Solid interiors get a few variants, picked
  by a hash of the position, so long walls don't visibly repeat.
- **Each tile kind is its own set of 16**, so each 6b tile kind adds a set
  when it's built. Collision is unchanged: tiles only draw the cells the
  level already has.
- **Features** (walls with traits) are drawn from a third tileset per
  theme, one quad per cell rather than on the dual grid, as platforms and
  pads are thinner than a cell. Each quad covers the part of its cell the
  feature fills, from the top down, and its tile depends on which of its
  four neighbours hold the same feature, so runs join and their ends are
  finished. One-way platforms are slim ledges of the theme's stone with
  faint chevrons pointing up; spring pads are a lit plate on coils;
  mud is a mottled sludge with a wet, bubbling top; and mirrors are
  silvered glass with bright streaks, framed in the theme's stone. A wall
  with traits draws only as its feature, and `just art preview` shows each
  one in its sample room.
- **Scenery is drawn as panels set back behind the walls**, on the same dual
  grid with a set of its own: a stone frame round leaded glass, with a rose
  window, a lit pane or broken panes in the solid variants. The glass is
  pale, so a panel's hue tints it strongly and it waits for that hue to be
  restored; a panel with no hue stays grey. Panels are darker than walls,
  draw nothing in open space (no grass or shadows), and treat beyond the
  room as empty, so they never read as something to bounce off.
- **Themes are drawn in full colour; a filter takes the colour out.** The
  shader finds each pixel's hue and how much of that hue has been restored,
  using the same rule as the wash (a secondary is limited by its less
  restored primary, and hues in between blend), and pulls the pixel towards
  grey by what's missing. A level starts grey, and the red parts of its
  theme return as red does, yellow once red and green are back. Only the
  world (walls, scenery, background) is filtered: the hero, enemies, pickups
  and phantoms keep their colours, so an enemy's colour always reads.
- **Themes follow the campaign order**, three levels per theme, so each
  stretch of the campaign feels like its own world; levels past the last
  theme keep it, and a single level or the dev levels use the first. A
  theme sets its wall and panel styles, palette, background colour and,
  later, background layers and props. Its background colour goes through
  the same filter as its art, so it starts grey too. In the dev panel,
  **Next level** steps through the campaign and the **theme** picker
  redraws the current level in any theme, to check each one.
  - **Quarry** (levels 1–3): sandstone blocks with moss and grass, blue
    crystal, leaded-glass panels and a blue-grey night. Mostly warm, so red
    brings the stone back, green the moss and blue the crystal.
  - **Crystal Caverns** (4–6): rough rock with worn edges, cracks and
    strata, stalactites under ledges and glowing fungus on top, cyan and
    magenta crystal, geode panels whose agate bands follow each panel's
    shape, and a teal-black background. Mostly cool, so the secondaries
    these levels bring in have something to light up.
  - **Observatory** (7–9): pale veined marble with gold trim on top,
    carved dentils underneath and gold and lapis inlays; stained-glass
    panels in every hue with a rose window, a star or a crescent moon; and
    deep indigo. Each pane returns with its own hue, and a panel's hue only
    tints it lightly, so the richest scene comes once everything is back.
  - **Foundry** (10–12, later): dark iron plates and rivets with glowing
    seams, and gratings for panels.
- **Sprites** for the hero, enemies, orbs, pickups and the prism are drawn
  in white and greys on one sheet (`assets/art/sprites.png`, painted by
  `just art`) and tinted in the game, so one sheet serves every colour.
  - **Hero:** a white ball with grey bands swirling from a hub. The bands
    turn with its spin under a separate layer of shading and highlight, lit
    from the top left, which stays put. It squashes flat on each bounce,
    then springs tall and settles, keeping its base on the floor.
  - **Motes** are twinkling sparks whose rays turn and flicker. **Lenses**
    are hexagonal crystals with lit facets round a flat table and a dark
    eye; a glint crosses each one every few seconds, with lenses out of
    step. **Orbs** glow, with two bright arcs circling them.
  - **Pickups** are dark glass bubbles in a lit rim, tinted by the
    power-up, round its white glyph. The **prism** is a glass triangle with
    a bright rim and lines to its corners.
  - Bursts, checkpoints and beams are still simple shapes.

## Sound

- **Style: mixed.** Retro synth (chip-style blips, zaps and sweeps) for
  core effects; glassy, shimmering modern synth for light, prism and colour
  moments.
- **Source: generated files.** A small Rust tool (`tools/sfx`, run with
  `just sfx`) synthesises every sound from parameters kept in code and
  writes the `.wav` files, which are committed. All sounds are original.
- **Playback** uses Bevy's built-in audio. The simulation emits game events
  (hits, defeats, pickups, lives); the presentation side plays sounds for
  them, so rules stay audio-free and testable.
- **Gentle stereo panning** by horizontal position relative to the camera,
  clamped so nothing plays hard left or right. Off-screen events are
  quieter, so waves can be heard arriving from either side.
- **Musical feedback in Lydian.** Pitched sounds come from one Lydian scale
  so overlapping effects sound in key. Red, green and blue map to the root,
  third and fifth (together a major triad, so white light is a consonant
  chord). The raised 4th, Lydian's colour note, is kept for shimmer accents
  such as pickups, so it never clashes.
- **Beam: a looping hum** while firing, with a short attack and release.
  Its pitch dips as energy runs low, as a warning before it cuts out.
- **Bounces are very soft**: a quiet low thump pitched up with the spin
  step, so speed is audible without becoming tiring.
- **First-pass events:**
  - Combat: beam start/stop, enemy hit, mote defeated, lens defeated, lens
    fires an orb, prism blocks an orb.
  - Hero: floor bounce, wall bump, life lost, respawn, invulnerability ends,
    room reset.
  - Power-ups: pickup appears, collected (rising chime, a distinct note per
    power-up), about to expire, abilities lost.
  - Prism/UI: freeze and unfreeze, energy empty, energy fully recharged.
- **Controls:** the player sets master, music and effects volumes in the
  settings (see "Screens and score"). The dev panel also has them, the
  beam's level within the effects, and buttons to audition each sound.
- **Music in four stems** that loop together: one 8-bar piece at 100 beats a
  minute in Lydian on C, so the effects sit over it, moving through C major
  7, D over C, E minor 7 and D over C. `tools/sfx` writes them at half the
  effects' sample rate to keep them small, with notes that ring past the
  end folded back into the start so the loop is seamless.
  - **Base (grey):** a soft pad and a low root. It always plays in a level.
  - **Red:** a plucked arpeggio in eighth notes, the rhythm.
  - **Green:** the melody, a warm lead with a little vibrato.
  - **Blue:** high bells on the chords' upper notes, echoing.
  - Each colour's stem swells in over a couple of seconds once that
    colour is restored, so the music fills out as the level does. The
    title, settings and the campaign's end play every stem; game over plays
    only the base. Menus don't stop the music; pausing lowers it.

## 3D

- **Undecided.** Keep game logic independent of rendering so a 2.5D or 3D
  presentation can be added once the 2D game is fun.

## Open questions

- Gauge size and light per enemy: tune by playtesting (starting at 10, with
  1 per mote and 4 per lens).
- Drop chance and how long a drop stays: tune by playtesting (currently
  20% and 8 s, adjustable in the dev panel).

## Iteration plan

| # | Goal | Scope |
|---|------|-------|
| 1 | Bounce feel | Test room with floor, walls and a low ceiling; fixed apex bounce; spin steps -5..+5; camera follow; debug overlay to tune apex, gravity and speed per step. |
| 2 | Prism | Auto-orbit, hold to freeze, orbit radius and speed tuning. |
| 3a | Laser beam | Hold-to-fire beam with an energy bar, ray-cast to the first wall in the travel direction, splitting into RGB beams through the prism; target dummies to test against. |
| 3b | Enemies | Motes (timed waves from the edges) and Lenses (placed turrets firing slow orbs); contact and shots cost lives; the prism blocks shots; lives and respawn. |
| 4 | Power-ups | Timed drops at the enemy's death spot, gap-filling drop choice, abilities kept until death: anti-gravity, rear shot, spread, rapid fire, piercing. |
| 4b | More power-ups | Mirror face, light nova, beam energy upgrades. |
| 4c | Sound effects | Game events from the simulation; `tools/sfx` generator; first-pass effects with Lydian colour notes, beam hum, stereo panning and dev panel volumes. |
| 5 | Light and colour | Coloured enemies feeding red, green and blue gauges; restored primaries and secondaries; phantom platforms; hidden things; greyscale-to-colour wash; exit gate opened by white; gauges HUD and sounds; test room with all of these. |
| 6a | Level format | TOML and ASCII format, default legend, rectangle merging; walls, phantoms, pickups, lenses, start, exit and checkpoints; waves and light settings; hot reload; validation and coloured viewer commands; test room converted to a level file; camera on both axes; exit loads the next level. |
| 6b-1 | Static tile kinds | One-way platforms, spring and mud bounce modifiers and mirror walls reflecting the beam, as legend traits on walls and phantoms; the checker handles them; feature art per theme; the workshop dev level. |
| 6b-2 | Changing tile kinds | Moving, crumbling and colour-breakable platforms, each with a reach rule for the checker. |
| 6c-1 | Solvability | Bounce reach model using the game physics; solvability checker over landings and restored colours; `level-check` proves levels can be finished; `campaign.toml` sets the play order. |
| 6c-2 | Generator | Seeded generator for wide and tall levels: rhythm terrain patterns, colour gates, lenses, checkpoints and hidden pickups; comfort and gate-bypass checks; per-stage random streams; `just levelgen` and an up-to-date check; a first campaign of eight generated levels. |
| 6c-3 | Generated features | Recipes opt into one-way platforms, springs, mud and mirrors; placement rules for wide and tall levels that keep gates honest; layouts must include every feature asked for; the campaign introduces them in turn. |
| 7a | Themed tiles | `just art` with a first theme; dual-grid edge and corner tiles for walls and scenery panels replacing flat rectangles; the hue-aware colour filter; `just art` and an up-to-date check. |
| 7b | Themes | More themes following the campaign order, each with its own background colour: Crystal Caverns, then the Observatory. |
| 7c | Sprites | A generated grey sprite sheet tinted in the game: the hero with a spinning surface under fixed shading and a squash on each bounce; twinkling motes, glinting lenses and glowing orbs; glass pickup bubbles round their glyphs; the glass prism. |
| 7d | Backdrops | Parallax background layers and props per theme. |
| 8-1 | Screens and score | Title, game over and campaign-finished screens with menus that stop the clock; continuing after a game over; points from game events, shown in play; the best score this session. |
| 8-2 | Music and audio | Four-stem music in Lydian that fills out as colours are restored; a settings screen for master, music and effects volumes, saved between runs. |
| 8b | Web build | Build for the browser (`wasm32`) and publish it to GitHub Pages with each release, so the game can be played from the project's page. WebGL2 first, as every current browser has it, including Safari on iOS; WebGPU where it pays later. A clock that works in the browser, the canvas fitted to the page, audio started by the first tap or key, no Quit in a browser tab, `wasm-opt` for size, `just web` to build and serve it locally, and a release job that deploys to Pages and attaches the build. |
| 8c | Touch controls | So phones and tablets can play, Safari on iOS first. On-screen buttons drawn by the game with Bevy UI rather than an HTML overlay, read through one input layer with the keyboard and gamepad: spin taps on the left; hold to fire and hold the prism on the right; hover buttons only with anti-gravity; pause; menus tapped directly. Multi-touch, so one thumb can hold fire while the other taps spin, and a finger sliding onto a button presses it. Shown after the first touch, hidden again on key or gamepad input. The page stops scrolling, zoom and the long-press menu (`touch-action`, the viewport meta tag), keeps buttons inside the safe areas, asks for landscape in portrait, and has a web manifest so Add to Home Screen plays without the browser's bars on iPhone, which lacks the Fullscreen API. To check on real devices: two-thumb play, audio starting on a tap, and home-screen mode. |
| 9+ | 3D | To be designed. |
