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

JA+ and TaystJK/jaPRO isolate private duels: the two duellers and everyone else
pass through each other. A JA+ 2.4 server leaves part of that to client-plugin
users, sending them a dueller as a solid player box flagged with `bolt1`, so on
those profiles prediction skips duelling players for a bystander and every player
or NPC but the opponent for a dueller
([duel_isolation.rs](../crates/jkr-client/src/duel_isolation.rs), applied where
[prediction_movers.rs](../crates/jkr-viewer/src/prediction_movers.rs) builds the
entity solids). Stock and unknown servers keep duellers solid.

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
