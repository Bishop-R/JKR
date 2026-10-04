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

### JA+ movement rules

On a JA+ server, prediction follows the rules
[pmove_japlus.rs](../crates/jkr-game-jka/src/pmove_japlus.rs) reads from
`CS_SERVERINFO`: the dialect and its `jp_cinfo` bits. JA+ is closed source; the
client side follows EternalJK's reimplementation of the JA+ client plugin and,
where that and a JA+ 2.4 server disagree, the server as replays observed it.
Stock and other servers, JKR's own included, keep the stock rules.

| Rule | When | Effect |
| --- | --- | --- |
| Flip kick | `jp_cinfo` flip kick (`jp_allowFlipKick`, default on) | Wall flips off a player beside, and a flip back off a player ahead when jumping at one while still rising (above 200); a run up a wall is unchanged when no player is there |
| Head slide | `jp_cinfo` head slide (`jp_slideOnPlayer`, default off) | Without it, standing on a player has ground friction instead of stock's frictionless slide |
| Yellow DFA | `jp_cinfo` yellow DFA (`jp_improveYellowDFA`, default on) | The medium flip over leaps 60 forward (stock 150) and neither turns nor locks the view |
| Wall run from flips | Every JA+ server | A run up a wall may start from the Force jump's forward, left and right flips, not only from a plain jump |
| Grip speed | Every JA+ server | Gripping keeps 0.8 of the run speed (stock 0.4; `jp_gripSpeedScale` default) |
| Melee buttons | Every JA+ server | With melee, an attack pressed with the holdable button is not cancelled |
| Taunts | Every JA+ server | Meditation keeps the player in place but the view free; other taunts leave movement and view free |
| Staff kick | Every JA+ server | A staff's alternate attack standing still is a front kick |

Not predicted: the options EternalJK never reads outside its `serverconfig`
listing (single-player attacks, new DFA, model scale, kata, auto replier, ledge
grab, alternate dimension, macro scan), the Jedi Outcast red DFA
(`jp_jk2RedDFA`, off by default), a changed `jp_gripSpeedScale` (not published)
and the animation holds for JA+'s extra GLA animations.

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
