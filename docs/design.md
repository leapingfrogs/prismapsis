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
  leads back to the first. For now the order is a list in `src/level/mod.rs`
  (test room, then tower); a campaign file comes with generated levels.
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
- **Traits** set in the legend:
  - bounce modifiers: a spring pad or mud, with a strength multiplier.
  - mirror: every surface reflects the beam by its angle of incidence.

### Level tooling

- `just level-show <file>` prints a level with ANSI colours per hue.
- `just level-check` reports every problem in the level files, and a test
  runs the same checks in CI. It checks that every character is in the
  legend, that the level has one start and an exit, and that legend entries
  and settings make sense. Tracks will be checked once moving platforms
  exist.
- A procedural generator that writes level files from a seed. Generated
  levels supply the first campaign.

## Damage

- **One hit loses a life** and returns the hero to the last checkpoint.

## Resolution and art

- The world is **1080 units tall** at any window size; width follows the
  aspect ratio.
- At 4K each world unit covers 2×2 pixels, so sprite art should be authored
  at **2× its in-world size** to stay sharp at 4K.

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
- **Controls:** master, effects and beam volume sliders in the dev panel,
  with buttons to audition each sound. Player-facing settings come with the
  game loop.
- **Music: not yet.** It comes later and will share the Lydian key, so the
  effects sit over it.

## 3D

- **Undecided.** Keep game logic independent of rendering so a 2.5D or 3D
  presentation can be added once the 2D game is fun.

## Open questions

- Procedural generator: which constraints guarantee a level is completable
  (reachable phantoms, enough enemies of each colour)?

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
| 6b | Tile kinds | One-way, moving, crumbling and colour-breakable platforms; spring and mud bounce modifiers; mirror walls reflecting the beam. |
| 6c | Campaign | Procedural level generator; a campaign file listing generated levels. |
| 7 | Game loop | Menus, pause, game over, score, music, audio settings. |
| 8+ | 3D | To be designed. |
