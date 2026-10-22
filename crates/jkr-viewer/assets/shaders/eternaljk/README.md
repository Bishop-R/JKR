# EternalJK shaders

Shader definitions that EternalJK's client options use and the retail game data lacks,
kept verbatim from EternalJK (commit a40e793, `assets/japro/shaders/japro_players.shader`),
which is licensed under the GNU General Public License version 2, as JKR is. They
reference only retail images. JKR adds them only where the loaded game and mods define
no shader of the same name (`ShaderCatalog::add_fallback_script`).

- `japro_players.shader` — `gfx/effects/raceShader` and `gfx/effects/duelShader`, the
  stand-ins `cg_stylePlayer` draws racers and duellists as.
