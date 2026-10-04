# JKR project wiki

This directory is the canonical project wiki for contributors and AI assistants.
It lives beside the source so implementation and documentation can be reviewed
and changed together. Begin with the status page, then the architecture.

| Page | Contents |
| --- | --- |
| [Status and priorities](status.md) | Current scope, verification and open work |
| [Architecture](architecture.md) | Crate ownership and compatibility boundaries |
| [Development](development.md) | Build, validation and contribution workflow |
| [Distributable packages](packages.md) | Windows/Linux ZIP layout, builds and startup checks |
| [Client](client.md) | Launch, configuration, content and common commands |
| [Dedicated server](server.md) | Hosting, configuration and local testing |
| [Rendering](rendering.md) | BSP rendering, lighting, UI and measurement |
| [Networking and gameplay](networking.md) | Protocol 26, prediction and server authority |

[AGENTS.md](../AGENTS.md) defines the documentation maintenance rules. Update the
page that owns a fact rather than adding a second account elsewhere. New facts
should link to code or identify their verification; proposals must remain labeled
as planned until implemented and checked.

For installation and quick launch commands, see the [project README](../README.md).
