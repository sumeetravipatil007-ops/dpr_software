# Open CAD Studio — DPR Software

Open CAD Studio is a cross-platform CAD and drafting platform built with Rust. This repository combines the core desktop/editor application with a custom DPR (Daily Progress Report / quantity measurement) plugin workflow for construction and measurement tasks.

The project supports desktop, browser-based, and plugin-driven workflows. The codebase is structured around a shared CAD document model, a plugin runtime interface, and optional platform-specific targets for Windows, Linux, macOS, and WebAssembly.

## Project summary

- Core CAD engine for DWG/DXF workflows and document editing
- Cross-platform desktop support for Windows, Linux, and macOS
- Web build support through WebAssembly and Trunk
- Native plugin runtime via versioned plugin APIs
- Custom DPR measurement plugin for quantity estimation and reporting
- Automation support through CLI, MCP, and JSON-based tooling

## System architectures

### 1. Desktop architecture

The desktop application is the primary native build and is designed around a Rust-based GUI and document engine.

Main components:

- `src/` — main application logic, commands, configuration, UI entry points, and app modules
- `crates/` — shared runtime and plugin support libraries
- `assets/` — icons, fonts, patterns, linetypes, and project resources
- `locales/` — translation catalogs for multi-language support
- `plugins/` — external and in-repo plugin packages

The host application includes:

- UI layer built with `iced`
- CAD scene/document model
- command and tool system
- DWG/DXF file support
- rendering pipeline and geometry handling
- plugin host runtime

### 2. Web architecture

The project supports a browser build using WebAssembly and JavaScript interop.

Key web-related pieces:

- `web/` — static web assets
- `index.html` and `web-app.html` — browser loading and app entry
- `build.rs` and `Trunk.toml` — app bundling and web build configuration
- `wasm32-unknown-unknown` target support for browser execution

This allows the CAD editor to run in a browser without requiring the full desktop native environment, while still preserving a common document logic layer.

### 3. Plugin architecture

The plugin system is designed around an external plugin host model. Plugins are not compiled into the core host; instead they are loaded as separate dynamic libraries and communicate with the host through a versioned API contract.

Relevant project files:

- `crates/ocs_plugin_api/` — plugin contract and host-facing API
- `docs/plugin-architecture.md` — plugin runtime design and rules
- `plugins/README.md` — plugin registry and marketplace notes

The architecture separates:

- host runtime (core app)
- plugin process (external add-on)
- optional domain logic library (engineering calculations)

This keeps the main application stable while allowing plugins to extend commands, tabs, and domain-specific workflows.

### 4. Automation and MCP architecture

The project also includes automation and AI-bridge capabilities.

- CLI conversion and export commands
- headless serve mode
- MCP endpoint for external tool integration
- JSON-line communication patterns for automation

This is especially useful for engineering workflows where CAD tools need to be called from scripts, plugins, or AI-driven integration layers.

## Supported platforms

The project targets the following environments:

| Platform | Status | Notes |
| --- | --- | --- |
| Windows | Primary desktop target | Installer, portable, native app support |
| Linux | Supported | Desktop app and native toolchain support |
| macOS | Supported | Apple Silicon focus |
| Web / WASM | Supported | Browser runtime via Trunk and WebAssembly |

## Installed and bundled plugins

The repository includes the following plugin package in the workspace:

### DPR Measurement plugin

Location: `plugins/dpr-measurement/`

This plugin is a prototype quantity and measurement extension for DPR-style workflows. It is designed to support:

- custom ribbon tabs and commands
- wall quantity measurement
- unit configuration
- status/dashboard summaries
- project measurement reporting

Example commands include:

- `DPR_SET_UNITS`
- `DPR_SET_WALL_HEIGHT`
- `DPR_SET_WALL_THICKNESS`
- `DPR_PICK_WALL`
- `DPR_SUMMARY`
- `DPR_DASHBOARD`
- `DPR_RESET`

This makes it suitable as a base for construction or civil quantity tracking features.

## Plugin registry overview

The repository also includes a curated registry of third-party plugins in `plugins/registry.json`. The currently registered ecosystem includes packages such as:

- Example Plugin
- Storm Sewer
- HydroComplete
- Land Survey
- MCP Bridge
- Python REPL Plugin

These plugins demonstrate the broader plugin ecosystem around the Open CAD Studio host and are compatible with the plugin contract model.

## Repository layout

```text
OpenCADStudio-main/
├── src/                     # Core application logic and modules
├── crates/                  # Shared libraries and API contracts
├── plugins/                 # Plugin packages and registry
│   ├── dpr-measurement/     # DPR measurement plugin
│   ├── README.md           # Plugin docs
│   └── registry.json       # Marketplace plugin registry
├── docs/                    # Architecture, plugin, automation, and release docs
├── locales/                 # Translation files
├── assets/                  # CAD assets, icons, fonts, patterns
├── web/                     # Web assets and browser resources
├── tests/                   # Regression and feature validation tests
├── Cargo.toml               # Rust workspace definition
├── build.rs                 # Build hook
├── README.md                # Project overview
├── LICENSE                  # Licensing terms
├── SECURITY.md              # Security policy
└── i18n.toml                # Internationalization config
```

## Getting started

### Prerequisites

- Rust toolchain (current stable)
- Git
- Platform build dependencies for your target OS

### Run the desktop app

```bash
cargo run
```

### Build the release binary

```bash
cargo build --release
```

### Run the web build

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk wasm-bindgen-cli
trunk serve
```

## Development notes

This project is designed for engineering workflows, drawing authoring, measurement, and extensibility. It combines a native CAD editor with modern plugin-based workflow design and automation support.

Important notes:

- Core app logic is Rust-first and cross-platform.
- Plugin compatibility is API-versioned and not tied to host internals.
- The host prefers external add-ons instead of hard-coding feature sets.
- The project is built for both desktop productivity and browser accessibility.

## License

This project is distributed under the GNU General Public License v3.0. See the `LICENSE` file for details.

## Support and contributions

This repository is intended for active development and extension. Contributions and plugin experiments are welcome, especially for tooling automation, quantity workflows, and new domain-specific CAD extensions.
