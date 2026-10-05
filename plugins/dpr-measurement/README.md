# DPR Measurement Plugin

This is a minimal prototype plugin built on top of Open CAD Studio.

It demonstrates the architecture for:

- adding a custom ribbon tab
- registering custom commands
- calculating wall quantities from length and height
- preparing the foundation for a DPR reporting workflow

## Build

```bash
cargo build --manifest-path plugins/dpr-measurement/Cargo.toml
```

## Goal of the prototype

This plugin is intentionally simple and meant to serve as the starting point for:

- block work quantity calculations
- plaster measurement rules
- floor area calculations
- DPR total aggregation

## Example commands

To measure a wall drawn in millimeters, enter `DPR_SET_UNITS MM` and
`DPR_SET_WALL_HEIGHT 2.8` in the command line, then click **Pick Wall** on the
DPR ribbon. Select either a closed rectangular polyline or one edge of a wall,
then select its opposite parallel edge. Paired edges must be within the
configured wall thickness and have aligned endpoints. The plugin measures the
long side, converts it to meters, and multiplies it by the configured height.
Picked walls are recorded as Block Work.

Useful commands:

- `DPR_SET_WALL_HEIGHT 2.8` — set the picked-wall height (default: 3 m)
- `DPR_SET_UNITS MM` — set units used by drawing coordinates (MM, CM, M, IN, FT; default: M)
- `DPR_SET_WALL_THICKNESS 200` — set maximum spacing between picked parallel edges (millimeters; default: 200)
- `DPR_PICK_WALL` — start selecting a line entity
- `DPR_SUMMARY` — review recorded measurements
- `DPR_DASHBOARD` — review project progress
- `DPR_ADD W-02 Internal_Plaster 4.25 2.8` — enter another activity manually
- `DPR_RESET` — clear measurements and daily entries
