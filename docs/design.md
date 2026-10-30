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
- **Walls** reverse horizontal movement.
- **Ceilings** (e.g. a low platform overhead) cut the bounce short: the hero
  rebounds downward without losing energy, so the apex is capped by the
  ceiling.

## The prism companion

- A prism **orbits the hero automatically** at a steady speed.
- **Holding a button freezes it** in place on its orbit, e.g. to park it in
  front as a shield or to line it up with incoming light.
- The prism **absorbs light** it sweeps through.

## Shooting

- The hero **fires automatically in the direction of travel**.
- Shots that pass through the prism are refracted into extra beams.

## Light and the spectrum

- Enemies release **red, green or blue light**.
- Light mixes **additively**, like real light:
  red + green = yellow, green + blue = cyan, red + blue = magenta,
  all three = white.
- Restoring a colour makes **world elements of that hue solid or visible**:
  platforms, doors and routes. Levels therefore gate progress by colour, and
  areas are revisited once new colours are restored.

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
  weighted random.

### Abilities

| Ability | Effect |
|---------|--------|
| Anti-gravity | Gravity stops and bouncing ends: up/down moves the hero directly, while horizontal speed still comes from the spin steps. For narrow spaces and precise positioning. Lasts until death. |
| Rear shot | Fire backwards as well as forwards. |
| Spread fire | Each shot fans out into several. |
| Rapid fire | Higher fire rate. |
| Piercing shots | Shots pass through enemies. |
| Mirror face | The prism reflects enemy shots back at them, so freezing it in place makes a shield. |
| Light nova | A burst that clears nearby enemies, usable again after a recharge (e.g. 30 s). |

Weapon power-ups are **separate pickups that stack**: rear shot, spread,
rapid fire and piercing all apply together once collected.

## Levels

- **Wide horizontal levels, scrolling both ways.** Negative spin travels
  left, so backtracking is natural.
- Progress means restoring a level's spectrum to open its exit.

## Damage

- **One hit loses a life** and returns the hero to the last checkpoint.

## 3D

- **Undecided.** Keep game logic independent of rendering so a 2.5D or 3D
  presentation can be added once the 2D game is fun.

## Open questions

- When a wall reverses the hero, does the spin step flip too, or does the next
  floor bounce send the hero back towards the wall?
- Does stored light persist across lives, or reset at a checkpoint?
- How are restored colours represented: per level, per world, or globally?
- Can secondary colours (yellow, cyan, magenta) come from enemies directly, or
  only from mixing?
- What does white light unlock?
- Drop chance and how long a drop stays: to be set by playtesting
  (starting guess: 20% and 8 s).

## Iteration plan

| # | Goal | Scope |
|---|------|-------|
| 1 | Bounce feel | Test room with floor, walls and a low ceiling; fixed apex bounce; spin steps -5..+5; camera follow; debug overlay to tune apex, gravity and speed per step. |
| 2 | Prism | Auto-orbit, hold to freeze, orbit radius and speed tuning. |
| 3 | Shooting and enemies | Auto-fire in travel direction, refraction through the prism, simple enemies that release light. |
| 4 | Power-ups | Timed drops at the enemy's death spot, gap-filling drop choice, abilities kept until death: anti-gravity, weapon upgrades, mirror face, light nova. |
| 5 | Light and colour | Absorbing light, additive mixing, colour-gated platforms and doors. |
| 6 | Levels | Two-way scrolling levels loaded from data, checkpoints, lives. |
| 7 | Game loop | Menus, pause, game over, score, audio. |
| 8+ | 3D | To be designed. |
