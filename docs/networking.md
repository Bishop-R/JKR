# Networking and gameplay

JKR targets Jedi Academy multiplayer protocol 26. Keep the network representation
compatible with ordinary servers and clients; internal engine modernization does
not authorize a wire change.

## Ownership

- [jkr-protocol](../crates/jkr-protocol/src/lib.rs) owns message encoding/decoding,
  gamestates, snapshots and legacy field representations. It has no sockets.
- [jkr-network](../crates/jkr-network/src/lib.rs) owns transport, discovery and the
  legacy endpoints. The server endpoint calls a `LegacyGameHost` interface.
- [jkr-client](../crates/jkr-client/src/lib.rs) owns live client session state,
  reliable commands, snapshot history and client-side integration.
- [jkr-game-jka](../crates/jkr-game-jka/src/lib.rs) owns game behavior shared by
  prediction and server simulation.
- [jkr-dedicated's bridge](../crates/jkr-dedicated/src/bridge.rs) connects those
  rules to native server storage and legacy replication. `jkr-server` itself
  owns worlds/entities, not the JKA game loop or network format.

Client [compatibility profiles](../crates/jkr-client/src/compat_profile.rs)
explicitly distinguish BaseJKA, JA+, TaystJK/jaPRO and unknown modules from
serverinfo. Profile detection and implemented adapter behavior are not a promise
that every feature of those servers is reproduced by JKR's dedicated server.

## JA+ grapple hook

On JA+ servers the client predicts the grapple hook (`+button12`) with the rules
in [pmove_grapple.rs](../crates/jkr-game-jka/src/pmove_grapple.rs); other
servers, JKR's own included, get no hook movement. The JA+ game fires the hook,
stores its anchor in `lastHitLoc` and flags the pulled player with `PMF_GRAPPLE`
(pm_flags bit 15). Each move then aims 16 units short of the anchor along the
view and replaces the velocity with a pull of 800 units/s (10 units/s per unit
inside 100 units), EternalJK's arithmetic and the JA+ 2.4 B7 module's, followed
by an air move whatever the ground or water below. A client-plugin user
(#108) who lets go of the key stays on the rope: the game clears the flag and
sets entity flag bit 16, and each move runs an air move and then swings the
player on a rope as long as the distance from the anchor to where the move began.
Use lets go of the hook in the game before the move, so a pull or hang with use
held is predicted as neither. Where EternalJK and the JA+ module differ (the pose
sets the legs only; a crouched player is pulled too; the pull always ends in an
air move), prediction follows the module. The game-side edges, the hook firing,
taking hold and letting go, arrive with the next snapshot and cannot be predicted.

## Parity requirements

Movement includes integer-millisecond user-command quantization. Validate common
steps of 8, 7, 4 and 3 ms, corresponding to the customary 125, 142, 250 and 333 FPS
caps. Do not smooth away simulation quirks that players depend on. Presentation
interpolation is separate from authoritative movement and command timing.

Shared prediction/server code prevents duplicate implementations but can still
share the same mistake. Establish gameplay behavior against OpenJK multiplayer
`codemp`, including relevant animation, events and timing. Wire changes require
byte-level reference evidence; reasoning from matching Rust structures is insufficient.

Check ordinary native and legacy client joins on isolated servers for integration.
A handshake or successful movement run does not verify downloads, every reliable
command, map restarts, all combat, vehicles or every game type. The current evidence
and open validation work are recorded in [status.md](status.md).
